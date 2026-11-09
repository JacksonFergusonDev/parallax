//! What each file inside a comic archive is: a page, its ComicInfo.xml, or
//! something to skip.

/// Most pages in any real comic; more suggests something that isn't one.
pub const MAX_ENTRIES: usize = 20_000;
/// Largest single page read into memory (a long webtoon strip can be tens
/// of megabytes; nothing legitimate is near this).
pub const MAX_PAGE_BYTES: u64 = 256 * 1024 * 1024;
/// Largest ComicInfo.xml read.
pub const MAX_COMIC_INFO_BYTES: u64 = 1024 * 1024;

const IMAGE_EXTENSIONS: [&str; 5] = ["jpg", "jpeg", "png", "gif", "webp"];
const JUNK_FILES: [&str; 3] = ["thumbs.db", "desktop.ini", ".ds_store"];

/// Why a file in the archive isn't used as a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// Not an image the app reads (text, XML other than ComicInfo, PDF…).
    NotAnImage,
    /// macOS or Windows leftovers, or a hidden file or folder.
    Hidden,
    /// An absolute path or one that climbs out with `..`.
    UnsafePath,
    /// Listed twice; the second copy is ignored.
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub name: String,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Page,
    ComicInfo,
    Skip(SkipReason),
}

/// Archive names with Windows separators turned into `/`.
pub(crate) fn normalise(name: &str) -> String {
    name.replace('\\', "/")
}

pub(crate) fn classify(name: &str) -> Kind {
    let name = normalise(name);
    if name.starts_with('/') || name.contains(':') || name.split('/').any(|c| c == "..") {
        return Kind::Skip(SkipReason::UnsafePath);
    }
    let components: Vec<&str> = name.split('/').filter(|c| !c.is_empty()).collect();
    let Some(base) = components.last() else {
        return Kind::Skip(SkipReason::NotAnImage);
    };
    let base_lower = base.to_lowercase();
    if components
        .iter()
        .any(|c| c.starts_with('.') || c.eq_ignore_ascii_case("__MACOSX"))
        || JUNK_FILES.contains(&base_lower.as_str())
    {
        return Kind::Skip(SkipReason::Hidden);
    }
    if base_lower == "comicinfo.xml" {
        return Kind::ComicInfo;
    }
    let is_image = base_lower
        .rsplit_once('.')
        .is_some_and(|(_, ext)| IMAGE_EXTENSIONS.contains(&ext));
    if is_image {
        Kind::Page
    } else {
        Kind::Skip(SkipReason::NotAnImage)
    }
}

/// The one folder every page sits in, when there is one (`Vol 1/p1.jpg`,
/// `Vol 1/p2.jpg` → `Vol 1`). Archives made by zipping a folder have it.
pub(crate) fn wrapper(names: &[String]) -> Option<String> {
    let first = names.first()?.split_once('/')?.0;
    names
        .iter()
        .all(|n| n.split_once('/').is_some_and(|(top, _)| top == first))
        .then(|| first.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_and_comic_info() {
        assert_eq!(classify("p001.JPG"), Kind::Page);
        assert_eq!(classify("Vol 1/p001.webp"), Kind::Page);
        assert_eq!(classify("ComicInfo.xml"), Kind::ComicInfo);
        assert_eq!(classify("Vol 1/comicinfo.XML"), Kind::ComicInfo);
    }

    #[test]
    fn skips_leftovers_and_other_files() {
        for name in [
            "__MACOSX/._p001.jpg",
            "._p001.jpg",
            ".hidden/p001.jpg",
            "Thumbs.db",
            ".DS_Store",
        ] {
            assert_eq!(classify(name), Kind::Skip(SkipReason::Hidden), "{name}");
        }
        for name in ["notes.txt", "scan.nfo", "book.pdf", "noextension"] {
            assert_eq!(classify(name), Kind::Skip(SkipReason::NotAnImage), "{name}");
        }
    }

    #[test]
    fn refuses_paths_that_escape() {
        for name in [
            "../evil.jpg",
            "a/../../evil.jpg",
            "/etc/evil.jpg",
            "C:\\evil.jpg",
            "a\\..\\evil.jpg",
        ] {
            assert_eq!(classify(name), Kind::Skip(SkipReason::UnsafePath), "{name}");
        }
    }

    #[test]
    fn dash_leading_names_are_ordinary_pages() {
        assert_eq!(classify("-01.jpg"), Kind::Page);
        assert_eq!(classify("--help.png"), Kind::Page);
    }

    #[test]
    fn finds_a_single_wrapper_folder() {
        let names = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            wrapper(&names(&["Vol 1/p1.jpg", "Vol 1/p2.jpg"])),
            Some("Vol 1".into())
        );
        assert_eq!(wrapper(&names(&["Vol 1/p1.jpg", "p2.jpg"])), None);
        assert_eq!(wrapper(&names(&["a/p1.jpg", "b/p1.jpg"])), None);
        assert_eq!(wrapper(&names(&["p1.jpg"])), None);
    }
}
