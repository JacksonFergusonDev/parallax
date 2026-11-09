//! Comics in the library: CBZ files stored byte for byte, with metadata from
//! their ComicInfo.xml or, failing that, their file name.

use std::fs;
use std::path::Path;

use erc_comic::{Cbz, ComicInfo, Manga, NameInfo, Role, parse_file_name, write_comic_info};
use erc_epub::{Contributor, Identifier};
use rusqlite::params;

use crate::format::{Direction, Format, Kind};
use crate::{
    Book, Error, ImportOutcome, Library, Result, book_dir, hash_file, io_err, json,
    normalize_authors, now, write_thumbnail,
};

/// `comic` to treat comics without ComicInfo's `Manga` field as Western
/// comics; otherwise they're manga, read right to left.
pub const COMICS_KIND: &str = "comics_kind";

/// What a comic's files say about it.
#[derive(Debug, Default)]
pub(crate) struct ComicMeta {
    pub title: String,
    pub authors: Vec<String>,
    pub contributors: Vec<Contributor>,
    pub series: Option<String>,
    pub series_index: Option<String>,
    pub tags: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub published: Option<String>,
    pub description: Option<String>,
    pub identifiers: Vec<Identifier>,
    pub manga: Option<Manga>,
}

/// ComicInfo first, the file name for whatever it lacks.
pub(crate) fn comic_meta(info: Option<&ComicInfo>, file_name: &str) -> ComicMeta {
    let name = parse_file_name(file_name);
    let empty = ComicInfo::new();
    let info = info.unwrap_or(&empty);

    let series = info
        .series()
        .map(str::to_owned)
        .or_else(|| (!name.series.is_empty()).then(|| name.series.clone()));
    let series_index = info
        .volume()
        .or(info.number())
        .map(str::to_owned)
        .or_else(|| name_index(&name));
    let title = info
        .title()
        .map(str::to_owned)
        .or_else(|| name.title.clone())
        .or_else(|| {
            let series = series.as_deref()?;
            Some(
                match (info.volume().or(name.volume.as_deref()), &series_index) {
                    (Some(v), _) => format!("{series} Vol. {v}"),
                    (None, Some(n)) => format!("{series} #{n}"),
                    (None, None) => series.to_owned(),
                },
            )
        })
        .unwrap_or_else(|| file_name.to_owned());

    // Writers are the authors; for a manga with no writer listed, the artist.
    let credits = info.credits();
    let names = |role: Role| -> Vec<String> {
        credits
            .iter()
            .filter(|c| c.role == role)
            .map(|c| c.name.clone())
            .collect()
    };
    let mut authors = names(Role::Writer);
    if authors.is_empty() {
        authors = names(Role::Penciller);
    }
    let authors = normalize_authors(&authors);
    let contributors = credits
        .iter()
        .filter(|c| !(c.role == Role::Writer || authors.contains(&c.name)))
        .map(|c| Contributor {
            name: c.name.clone(),
            role: marc_role(c.role).to_owned(),
        })
        .fold(Vec::<Contributor>::new(), |mut all, c| {
            if !all.iter().any(|a| a.name == c.name && a.role == c.role) {
                all.push(c);
            }
            all
        });

    let split = |field: &str| -> Vec<String> {
        info.get(field)
            .into_iter()
            .flat_map(|v| v.split(','))
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let mut tags = split("Genre");
    for tag in split("Tags") {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    let published = info
        .year()
        .map(
            |y| match info.get("Month").and_then(|m| m.parse::<u8>().ok()) {
                Some(m @ 1..=12) => format!("{y}-{m:02}"),
                _ => y.to_string(),
            },
        )
        .or_else(|| name.year.map(|y| y.to_string()));
    let identifiers = info
        .get("GTIN")
        .map(|g| Identifier {
            scheme: Some("isbn".into()),
            value: g.to_owned(),
        })
        .into_iter()
        .collect();
    ComicMeta {
        title,
        authors,
        contributors,
        series,
        series_index,
        tags,
        language: info.language().map(str::to_owned),
        publisher: info.publisher().map(str::to_owned),
        published,
        description: info.summary().map(str::to_owned),
        identifiers,
        manga: Some(info.manga()).filter(|m| *m != Manga::Unknown),
    }
}

fn name_index(name: &NameInfo) -> Option<String> {
    name.volume
        .clone()
        .or_else(|| name.chapter.clone())
        .or_else(|| name.number.clone())
}

/// ComicInfo roles as MARC relator codes, as EPUB contributors use.
fn marc_role(role: Role) -> &'static str {
    match role {
        Role::Writer => "aut",
        Role::Penciller | Role::Inker => "art",
        Role::Colorist => "clr",
        Role::Letterer => "ctb",
        Role::CoverArtist => "cov",
        Role::Editor => "edt",
        Role::Translator => "trl",
    }
}

/// Kind and direction: ComicInfo's `Manga` field when it has one, else the
/// library's default.
fn kind_and_direction(manga: Option<Manga>, default_is_manga: bool) -> (Kind, Direction) {
    match manga {
        Some(Manga::YesAndRightToLeft) => (Kind::Manga, Direction::Rtl),
        // Manga already in left-to-right order, per the schema.
        Some(Manga::Yes) => (Kind::Manga, Direction::Ltr),
        Some(Manga::Unknown) | None if default_is_manga => (Kind::Manga, Direction::Rtl),
        Some(Manga::No | Manga::Unknown) | None => (Kind::Comic, Direction::Ltr),
    }
}

/// A comic archive rather than an EPUB: a zip with no EPUB container, or a
/// RAR or 7z archive. Unreadable files aren't comics (the EPUB import then
/// reports why).
pub(crate) fn comic_format(path: &Path) -> Option<erc_comic::Format> {
    let format = erc_comic::sniff(path).ok()?;
    if format != erc_comic::Format::Zip {
        return Some(format);
    }
    let zip = zip::ZipArchive::new(fs::File::open(path).ok()?).ok()?;
    let is_epub = zip.index_for_name("META-INF/container.xml").is_some()
        || zip.index_for_name("mimetype").is_some();
    (!is_epub).then_some(format)
}

impl Library {
    /// Imports a comic in any format the comic crate reads: CBZ as it is,
    /// CBR and CB7 converted to CBZ and identified by the original.
    pub(crate) fn import_any_comic(
        &mut self,
        path: &Path,
        format: erc_comic::Format,
    ) -> Result<ImportOutcome> {
        if format == erc_comic::Format::Zip {
            return self.import_comic(path, path);
        }
        let (hash, _) = hash_file(path)?;
        if let Some(book) = self.find_by_hash(&hash)? {
            return Ok(ImportOutcome::AlreadyInLibrary { book });
        }
        let work = tempfile::tempdir().map_err(|e| io_err(path, e))?;
        let cbz = work.path().join("book.cbz");
        erc_comic::convert_to_cbz(path, &cbz)?;
        self.import_comic(&cbz, path)
    }

    /// Imports the CBZ at `cbz`, identified by `original` (a CBR or CB7 it
    /// was converted from, or itself), whose name is shown and parsed for
    /// metadata.
    pub fn import_comic(&mut self, cbz: &Path, original: &Path) -> Result<ImportOutcome> {
        let (hash, _) = hash_file(original)?;
        if let Some(book) = self.find_by_hash(&hash)? {
            return Ok(ImportOutcome::AlreadyInLibrary { book });
        }
        let mut comic = Cbz::open(cbz)?;
        let original_name = original
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let meta = comic_meta(comic.comic_info(), &original_name);
        let default_is_manga = self.setting(COMICS_KIND)?.as_deref() != Some("comic");
        let (kind, direction) = kind_and_direction(meta.manga, default_is_manga);
        let pages = comic.pages().len() as u64;
        let cover = comic.read_page(comic.cover_index()).ok();
        let size = fs::metadata(cbz).map_err(|e| io_err(cbz, e))?.len();
        let now = now();

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO books (title, authors, series, series_index, tags, language, publisher, description,
                                identifiers, file_hash, file_size, original_name, has_cover, added_at, modified_at,
                                contributors, published, words, pages, pages_estimated, format, kind, direction)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0, ?13, ?13, ?14, ?15, NULL, ?16, 0, ?17, ?18, ?19)",
            params![
                meta.title,
                json(&meta.authors),
                meta.series,
                meta.series.as_ref().and(meta.series_index.as_ref()),
                json(&meta.tags),
                meta.language,
                meta.publisher,
                meta.description,
                json(&meta.identifiers),
                hash,
                size,
                original_name,
                now,
                json(&meta.contributors),
                meta.published,
                pages,
                Format::Cbz,
                kind,
                direction,
            ],
        )?;
        let id = tx.last_insert_rowid();
        let dir = book_dir(&self.root, id);
        let stored = (|| -> Result<bool> {
            fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
            let dest = dir.join("book.cbz");
            fs::copy(cbz, &dest).map_err(|e| io_err(&dest, e))?;
            Ok(cover.is_some_and(|c| write_thumbnail(&c, &dir.join("cover.jpg"))))
        })();
        match stored {
            Ok(has_cover) => {
                tx.execute(
                    "UPDATE books SET has_cover = ?1 WHERE id = ?2",
                    params![has_cover, id],
                )?;
                tx.commit()?;
            }
            Err(e) => {
                let _ = fs::remove_dir_all(&dir);
                return Err(e);
            }
        }
        let book = self.book(id)?;
        let possible_duplicate_of = self.find_similar(&book)?;
        Ok(ImportOutcome::Added {
            book,
            possible_duplicate_of,
        })
    }

    /// Counts spreads from the stored page headers.
    pub fn comic_spreads(&self, id: i64) -> Result<usize> {
        let book = self.book(id)?;
        if book.format != Format::Cbz {
            return Err(Error::NotAComic(id));
        }
        let mut comic = Cbz::open(&self.book_file(id))?;
        Ok(erc_comic::shape::Shapes::count(
            (0..comic.pages().len()).map(|i| comic.page_dimensions(i).ok().flatten()),
        )
        .spreads)
    }

    /// Changes whether a comic is manga or a Western comic, and which way its
    /// pages turn.
    pub fn set_comic_kind(&mut self, id: i64, kind: Kind, direction: Direction) -> Result<Book> {
        let book = self.book(id)?;
        if book.format != Format::Cbz || kind == Kind::Book {
            return Err(Error::NotAComic(id));
        }
        self.conn.execute(
            "UPDATE books SET kind = ?1, direction = ?2, modified_at = ?3 WHERE id = ?4",
            params![kind, direction, now(), id],
        )?;
        self.book(id)
    }

    /// Copies a comic to `out` with its library metadata written into its
    /// ComicInfo.xml, keeping every field the library doesn't manage.
    pub(crate) fn export_comic(&self, book: &Book, out: &Path) -> Result<()> {
        let src = self.book_file(book.id);
        let existing = Cbz::open(&src)?.comic_info().cloned();
        fs::copy(&src, out).map_err(|e| io_err(out, e))?;
        let mut info = existing.unwrap_or_default();
        apply_to_comic_info(book, &mut info);
        write_comic_info(out, &info)?;
        Ok(())
    }

    /// The comic's cover page, at full size.
    pub(crate) fn comic_cover(&self, id: i64) -> Result<Option<Vec<u8>>> {
        let mut comic = Cbz::open(&self.book_file(id))?;
        let index = comic.cover_index();
        Ok(Some(comic.read_page(index)?))
    }
}

/// The library's metadata as ComicInfo fields.
fn apply_to_comic_info(book: &Book, info: &mut ComicInfo) {
    let joined = |v: Vec<&str>| (!v.is_empty()).then(|| v.join(", "));
    info.set("Title", Some(&book.title));
    info.set("Series", book.series.as_deref());
    // A whole number is a volume; anything else (12.5) an issue or chapter.
    let index = book.series_index.as_deref();
    if index.is_some_and(|i| i.parse::<u32>().is_ok()) {
        info.set("Volume", index);
    } else {
        info.set("Number", index);
    }
    let contributors = book.contributors.as_deref().unwrap_or_default();
    let by = |codes: &[&str]| {
        joined(
            contributors
                .iter()
                .filter(|c| codes.contains(&c.role.as_str()))
                .map(|c| c.name.as_str())
                .collect(),
        )
    };
    info.set(
        "Writer",
        joined(book.authors.iter().map(String::as_str).collect()).as_deref(),
    );
    if let Some(artists) = by(&["art", "ill"]) {
        info.set("Penciller", Some(&artists));
    }
    for (field, code) in [
        ("Colorist", "clr"),
        ("CoverArtist", "cov"),
        ("Editor", "edt"),
        ("Translator", "trl"),
    ] {
        if let Some(names) = by(&[code]) {
            info.set(field, Some(&names));
        }
    }
    info.set("Summary", book.description.as_deref());
    if info.get("Web").is_none()
        && let Some(source) = book
            .identifiers
            .iter()
            .find(|i| matches!(i.scheme.as_deref(), Some("metron-url" | "mangainfo-url")))
    {
        info.set("Web", Some(&source.value));
    }
    info.set("Publisher", book.publisher.as_deref());
    info.set("LanguageISO", book.language.as_deref());
    info.set(
        "Genre",
        joined(book.tags.iter().map(String::as_str).collect()).as_deref(),
    );
    info.set(
        "Manga",
        Some(match (book.kind, book.direction) {
            (Kind::Manga, Direction::Rtl) => "YesAndRightToLeft",
            (Kind::Manga, Direction::Ltr) => "Yes",
            _ => "No",
        }),
    );
    if let Some(year) = book.published.as_deref().and_then(|p| p.get(..4)) {
        info.set("Year", Some(year));
    }
    info.set("PageCount", book.pages.map(|p| p.to_string()).as_deref());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_fill_in_without_comic_info() {
        let meta = comic_meta(None, "Berserk v01 (2003) (Digital) (danke-Empire).cbz");
        assert_eq!(meta.title, "Berserk Vol. 1");
        assert_eq!(meta.series.as_deref(), Some("Berserk"));
        assert_eq!(meta.series_index.as_deref(), Some("1"));
        assert_eq!(meta.published.as_deref(), Some("2003"));
        assert!(meta.authors.is_empty());
        assert_eq!(meta.manga, None);

        let issue = comic_meta(None, "Saga #12.cbr");
        assert_eq!(issue.title, "Saga #12");
        let plain = comic_meta(None, "Akira.cbz");
        assert_eq!(plain.title, "Akira");
        assert_eq!(plain.series.as_deref(), Some("Akira"));
    }

    #[test]
    fn comic_info_wins_and_credits_split_into_roles() {
        let info = ComicInfo::parse(
            "<ComicInfo><Series>Monster</Series><Volume>3</Volume>\
             <Writer>Naoki Urasawa</Writer><Penciller>Naoki Urasawa</Penciller>\
             <Colorist>A Colourist</Colorist><Translator>Camellia Nieh</Translator>\
             <Genre>Thriller, Mystery</Genre><Tags>Mystery, Seinen</Tags>\
             <Year>2006</Year><Month>4</Month><Manga>YesAndRightToLeft</Manga></ComicInfo>",
        )
        .unwrap();
        let meta = comic_meta(Some(&info), "monster_03.cbz");
        assert_eq!(meta.title, "Monster Vol. 3");
        assert_eq!(meta.authors, ["Naoki Urasawa"]);
        let roles: Vec<(&str, &str)> = meta
            .contributors
            .iter()
            .map(|c| (c.name.as_str(), c.role.as_str()))
            .collect();
        assert_eq!(roles, [("A Colourist", "clr"), ("Camellia Nieh", "trl")]);
        assert_eq!(meta.tags, ["Thriller", "Mystery", "Seinen"]);
        assert_eq!(meta.published.as_deref(), Some("2006-04"));
        assert_eq!(meta.manga, Some(Manga::YesAndRightToLeft));
    }

    #[test]
    fn manga_artists_stand_in_for_missing_writers() {
        let info = ComicInfo::parse("<ComicInfo><Penciller>Kentaro Miura</Penciller></ComicInfo>")
            .unwrap();
        let meta = comic_meta(Some(&info), "x.cbz");
        assert_eq!(meta.authors, ["Kentaro Miura"]);
        assert!(meta.contributors.is_empty());
    }

    #[test]
    fn kind_follows_comic_info_then_the_default() {
        use Direction::{Ltr, Rtl};
        assert_eq!(
            kind_and_direction(Some(Manga::YesAndRightToLeft), false),
            (Kind::Manga, Rtl)
        );
        assert_eq!(
            kind_and_direction(Some(Manga::Yes), false),
            (Kind::Manga, Ltr)
        );
        assert_eq!(
            kind_and_direction(Some(Manga::No), true),
            (Kind::Comic, Ltr)
        );
        assert_eq!(kind_and_direction(None, true), (Kind::Manga, Rtl));
        assert_eq!(kind_and_direction(None, false), (Kind::Comic, Ltr));
    }
}
