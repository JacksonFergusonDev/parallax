//! Send to Kindle by email: each library book goes as an EPUB attachment to
//! the user's `@kindle.com` address, where Amazon converts it and delivers it
//! to their Kindles. No cable, so it works with any Kindle.
//!
//! The sending address must be on the Amazon account's Approved Personal
//! Document E-mail List, or Amazon drops the message. Amazon accepts up to
//! 50 MB per email, so each book goes in its own email. The SMTP password is
//! kept in the system keychain, never in the library.

use lettre::message::header::ContentType;
use lettre::message::{Attachment, Mailbox, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};
use parallax_library::{Book, Format, Library};
use serde::{Deserialize, Serialize};

use crate::send::{Progress, SendOutcome, SentInfo, run_batch};

/// Settings keys. The password is in the keychain under [`KEYCHAIN_SERVICE`].
pub const KINDLE_EMAIL_TO: &str = "kindle_email_to";
pub const SMTP_HOST: &str = "smtp_host";
pub const SMTP_PORT: &str = "smtp_port";
pub const SMTP_USER: &str = "smtp_user";
pub const SMTP_FROM: &str = "smtp_from";
/// "starttls" (the default) or "tls".
pub const SMTP_SECURITY: &str = "smtp_security";
pub const KEYCHAIN_SERVICE: &str = "Parallax SMTP";

/// Amazon's limit for one email, attachments included.
pub const MAX_EMAIL_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmailSettings {
    /// The Kindle's address, e.g. `name_abc@kindle.com`.
    pub to: String,
    pub host: String,
    pub port: Option<u16>,
    pub user: String,
    /// The approved sender; the SMTP user when empty.
    pub from: String,
    /// Implicit TLS (usually port 465) instead of STARTTLS (587).
    pub tls: bool,
}

impl EmailSettings {
    pub fn load(library: &Library) -> Self {
        let get = |key| library.setting(key).ok().flatten().unwrap_or_default();
        Self {
            to: get(KINDLE_EMAIL_TO),
            host: get(SMTP_HOST),
            port: get(SMTP_PORT).parse().ok(),
            user: get(SMTP_USER),
            from: get(SMTP_FROM),
            tls: get(SMTP_SECURITY) == "tls",
        }
    }

    pub fn save(&self, library: &Library) -> parallax_library::Result<()> {
        let set = |key, value: &str| {
            let value = value.trim();
            library.set_setting(key, (!value.is_empty()).then_some(value))
        };
        set(KINDLE_EMAIL_TO, &self.to)?;
        set(SMTP_HOST, &self.host)?;
        set(SMTP_PORT, &self.port.map_or_default(|p| p.to_string()))?;
        set(SMTP_USER, &self.user)?;
        set(SMTP_FROM, &self.from)?;
        set(SMTP_SECURITY, if self.tls { "tls" } else { "" })
    }

    fn sender(&self) -> &str {
        if self.from.trim().is_empty() {
            self.user.trim()
        } else {
            self.from.trim()
        }
    }

    /// What's missing before an email can be sent, in words for the user.
    pub fn problem(&self) -> Option<String> {
        if !self.to.trim().to_lowercase().ends_with("@kindle.com")
            && !self.to.trim().to_lowercase().ends_with("@free.kindle.com")
        {
            return Some(
                "Add your Kindle's email address (it ends in @kindle.com) in Settings › Kindle."
                    .into(),
            );
        }
        if self.host.trim().is_empty() || self.user.trim().is_empty() {
            return Some(
                "Add your email account's outgoing mail server and user name in Settings › Kindle."
                    .into(),
            );
        }
        if self.sender().parse::<Mailbox>().is_err() {
            return Some(
                "The sending address in Settings › Kindle isn't a valid email address.".into(),
            );
        }
        None
    }
}

fn keychain(user: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, user).map_err(|e| e.to_string())
}

/// Stores the SMTP password in the keychain, or removes it when empty.
pub fn set_password(user: &str, password: &str) -> Result<(), String> {
    let entry = keychain(user)?;
    if password.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    } else {
        entry.set_password(password).map_err(|e| e.to_string())
    }
}

/// Whether a password is stored for `user`.
pub fn has_password(user: &str) -> bool {
    !user.is_empty()
        && keychain(user)
            .and_then(|e| e.get_password().map_err(|e| e.to_string()))
            .is_ok()
}

fn transport(settings: &EmailSettings, password: String) -> Result<SmtpTransport, String> {
    let host = settings.host.trim();
    let builder = if settings.tls {
        SmtpTransport::relay(host)
    } else {
        SmtpTransport::starttls_relay(host)
    }
    .map_err(|e| e.to_string())?;
    let builder = match settings.port {
        Some(port) => builder.port(port),
        None => builder,
    };
    Ok(builder
        .credentials(Credentials::new(settings.user.trim().to_owned(), password))
        .build())
}

/// The email for one book.
pub fn message(settings: &EmailSettings, book: &Book, epub: Vec<u8>) -> Result<Message, String> {
    let from: Mailbox = settings
        .sender()
        .parse()
        .map_err(|e| format!("sending address: {e}"))?;
    let to: Mailbox = settings
        .to
        .trim()
        .parse()
        .map_err(|e| format!("Kindle address: {e}"))?;
    let name = attachment_name(book);
    Message::builder()
        .from(from)
        .to(to)
        .subject(book.title.clone())
        .multipart(
            MultiPart::mixed()
                .singlepart(SinglePart::plain(String::from("Sent from Parallax.")))
                .singlepart(Attachment::new(name).body(
                    epub,
                    ContentType::parse("application/epub+zip").map_err(|e| e.to_string())?,
                )),
        )
        .map_err(|e| e.to_string())
}

/// `Title - Author.epub`, safe as an attachment name.
pub fn attachment_name(book: &Book) -> String {
    parallax_device_kindle::transfer::file_name(&book.title, &book.authors, "epub")
}

/// Emails each library book to the Kindle address. Comics aren't sent:
/// Send to Kindle doesn't take CBZ.
pub fn send_books(
    library: &Library,
    settings: &EmailSettings,
    ids: &[i64],
    on_progress: impl FnMut(Progress),
) -> Result<Vec<SendOutcome>, String> {
    if let Some(problem) = settings.problem() {
        return Err(problem);
    }
    let password = keychain(settings.user.trim())?
        .get_password()
        .map_err(|_| "Add your email account's password in Settings › Kindle.".to_owned())?;
    let mailer = transport(settings, password)?;
    let books: Vec<Result<Book, String>> = ids
        .iter()
        .map(|&id| library.book(id).map_err(|e| e.to_string()))
        .collect();
    let labels: Vec<String> = books
        .iter()
        .zip(ids)
        .map(|(b, id)| {
            b.as_ref()
                .map_or_else(|_| format!("Book {id}"), |b| b.title.clone())
        })
        .collect();
    Ok(run_batch(&labels, on_progress, |i, stage| {
        let result = books[i].clone().and_then(|book| {
            if book.format == Format::Cbz {
                return Err("Send to Kindle doesn't take comics (CBZ)".to_owned());
            }
            stage(&|index, total, file| Progress::Preparing { index, total, file });
            let work = tempfile::tempdir().map_err(|e| e.to_string())?;
            let epub = work.path().join("book.epub");
            library.export(book.id, &epub).map_err(|e| e.to_string())?;
            let data = std::fs::read(&epub).map_err(|e| e.to_string())?;
            let bytes = data.len() as u64;
            // Base64 makes attachments a third larger.
            if bytes * 4 / 3 > MAX_EMAIL_BYTES {
                return Err(format!(
                    "too large to email ({} MB; Amazon takes up to 50 MB)",
                    bytes / 1_000_000
                ));
            }
            stage(&|index, total, file| Progress::Copying {
                index,
                total,
                file,
                bytes: 0,
                total_bytes: bytes,
            });
            let email = message(settings, &book, data)?;
            mailer
                .send(&email)
                .map_err(|e| format!("couldn't send the email: {e}"))?;
            Ok(SentInfo {
                device_path: settings.to.trim().to_owned(),
                content_id: String::new(),
                bytes,
                skipped_pages: Vec::new(),
                details_only: false,
            })
        });
        SendOutcome {
            source: labels[i].clone(),
            book_id: Some(ids[i]),
            result,
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> EmailSettings {
        EmailSettings {
            to: "reader_x1@kindle.com".into(),
            host: "smtp.example.com".into(),
            port: Some(587),
            user: "me@example.com".into(),
            from: String::new(),
            tls: false,
        }
    }

    #[test]
    fn problems_in_words() {
        assert_eq!(settings().problem(), None);
        let mut s = settings();
        s.to = "me@gmail.com".into();
        assert!(s.problem().unwrap().contains("@kindle.com"));
        let mut s = settings();
        s.host.clear();
        assert!(s.problem().unwrap().contains("server"));
        let mut s = settings();
        s.user = "not an address".into();
        assert!(s.problem().unwrap().contains("valid"));
        s.from = "me@example.com".into();
        assert_eq!(s.problem(), None);
    }

    #[test]
    fn builds_message_with_epub() {
        let mut book = test_book();
        book.title = "The Enchiridion".into();
        book.authors = vec!["Epictetus".into()];
        let email = message(&settings(), &book, b"PK epub".to_vec()).unwrap();
        let raw = String::from_utf8_lossy(&email.formatted()).into_owned();
        assert!(raw.contains("To: reader_x1@kindle.com"));
        assert!(raw.contains("From: me@example.com"));
        assert!(raw.contains("application/epub+zip"));
        assert!(raw.contains("The Enchiridion - Epictetus.epub"));
    }

    fn test_book() -> Book {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::open(dir.path()).unwrap();
        let epub = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/epub/minimal.epub");
        match lib.import(&epub).unwrap() {
            parallax_library::ImportOutcome::Added { book, .. }
            | parallax_library::ImportOutcome::AlreadyInLibrary { book } => book,
        }
    }
}
