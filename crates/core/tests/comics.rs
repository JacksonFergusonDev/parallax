//! Comics through the app's own paths: adding one, and Check library.

use std::fs::File;
use std::io::{Cursor, Write};
use std::path::Path;

use parallax_convert::amazon::InProcess;
use parallax_core::health::{Check, check_library};
use parallax_core::ingest::import_file;
use parallax_library::{Format, ImportOutcome, Library};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::GrayImage::new(width, height)
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn comics_are_added_through_the_normal_import() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let path = dir.path().join("Vinland Saga v03.cbz");
    write_zip(&path, &[("01.png", &png(20, 30)), ("02.png", &png(20, 30))]);
    let ImportOutcome::Added { book, .. } = import_file(&mut library, &InProcess, &path).unwrap()
    else {
        panic!("expected a new book");
    };
    assert_eq!(book.format, Format::Cbz);
    assert_eq!(book.title, "Vinland Saga Vol. 3");
    assert_eq!(book.pages, Some(2));
}

#[test]
fn check_library_reads_comics_as_comics() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let page = png(1300, 1900);
    let clean = dir.path().join("Clean v01.cbz");
    write_zip(&clean, &[("01.png", &page), ("02.png", &page)]);
    let messy = dir.path().join("Messy v01.cbz");
    write_zip(
        &messy,
        &[
            ("01.png", &page),
            ("02.png", b"not really a png"),
            (".DS_Store", b"junk"),
            ("release.nfo", b"scanned by"),
        ],
    );
    let id = |outcome| match outcome {
        ImportOutcome::Added { book, .. } => book.id,
        ImportOutcome::AlreadyInLibrary { .. } => panic!("expected a new book"),
    };
    let clean_id = id(library.import(&clean).unwrap());
    let messy_id = id(library.import(&messy).unwrap());

    let findings = check_library(&library, |_, _| {}).unwrap();
    let about = |book: i64| -> Vec<Check> {
        findings
            .iter()
            .filter(|f| f.book_ids.contains(&book))
            .map(|f| f.check)
            .collect()
    };
    // Comics without ComicInfo have no author; nothing else is wrong.
    assert_eq!(about(clean_id), [Check::NoAuthor]);
    let messy_checks = about(messy_id);
    for check in [Check::DamagedPages, Check::Clutter, Check::Unmanifested] {
        assert!(
            messy_checks.contains(&check),
            "{check:?} in {messy_checks:?}"
        );
    }
    assert!(!messy_checks.contains(&Check::Unreadable));
    let damaged = findings
        .iter()
        .find(|f| f.check == Check::DamagedPages)
        .unwrap();
    assert_eq!(damaged.detail, "02.png");
}

#[test]
fn comic_enrichment_uses_the_embedded_issue_and_preserves_local_details() {
    use parallax_core::enhance::{self, Accepted, Details, DeviceClues, METRON_TOKEN};
    use parallax_metadata::{Fetch, FetchError};

    struct Metron;
    impl Fetch for Metron {
        fn get(&self, url: &str, _: &[(&str, &str)]) -> Result<String, FetchError> {
            panic!("unexpected anonymous request {url}")
        }

        fn get_authenticated(
            &self,
            url: &str,
            params: &[(&str, &str)],
            token: &str,
        ) -> Result<String, FetchError> {
            assert_eq!(token, "test");
            Ok(match url {
                "https://metron.cloud/api/issue/" => {
                    assert!(params.contains(&("series_name", "Example")));
                    assert!(params.contains(&("number", "12")));
                    assert!(params.contains(&("series_volume", "2")));
                    r#"{"next":null,"results":[{"id":41,"issue":"Example (2018) #12","series":{"name":"Example"}}]}"#
                }
                "https://metron.cloud/api/issue/41/" => r#"{"id":41,"series":{"name":"Example","genres":[{"name":"Fantasy"}]},"number":"12","publisher":{"name":"Online Press"},"desc":"Online summary","desc_is_solicitation":false,"resource_url":"https://metron.cloud/issue/example-2/12/"}"#,
                _ => panic!("unexpected request {url}"),
            }.into())
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    library.set_setting(METRON_TOKEN, Some("test")).unwrap();
    let path = dir.path().join("Example v2 #12.cbz");
    write_zip(&path, &[
        ("1.png", &png(10, 20)),
        ("ComicInfo.xml", b"<ComicInfo><Series>Example</Series><Volume>2</Volume><Number>12</Number><Manga>No</Manga><Writer>Local Author</Writer><Publisher>Local Press</Publisher></ComicInfo>"),
    ]);
    let ImportOutcome::Added { book, .. } = library.import(&path).unwrap() else {
        panic!("expected a comic");
    };
    let books = [book];
    let options = enhance::options(&library, &books);
    let proposed = enhance::propose(
        &books,
        &options,
        &Metron,
        &DeviceClues::default(),
        |_, _, _| {},
    );
    let proposed = &proposed[0];
    let details = proposed.details.as_ref().unwrap();
    assert!(!details.confident);
    assert!(details.values.authors.is_empty());
    assert!(details.values.publisher.is_none());

    let accepted = [Accepted {
        book_id: books[0].id,
        details: Some(Details {
            authors: vec!["Must not replace a saved author".into()],
            publisher: Some("Must not replace a saved publisher".into()),
            ..details.values.clone()
        }),
        record: proposed.record.clone(),
        ..Accepted::default()
    }];
    let applied = enhance::apply(&mut library, &accepted).unwrap();
    assert_eq!(applied[0].authors, ["Local Author"]);
    assert_eq!(applied[0].publisher.as_deref(), Some("Local Press"));
    assert_eq!(applied[0].description.as_deref(), Some("Online summary"));
    assert!(
        applied[0]
            .identifiers
            .iter()
            .any(|i| i.scheme.as_deref() == Some("metron") && i.value == "41")
    );
}
