use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use erc_epub::{Contributor, Epub, MetadataEdit};
use erc_library::{Error, ImportOutcome, Library};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(width, height, image::Rgb([200, 60, 40]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// An EPUB2 with the given metadata; `salt` changes the bytes without
/// changing the metadata.
fn epub(
    dir: &Path,
    name: &str,
    title: &str,
    creator: &str,
    isbn: Option<&str>,
    salt: &str,
) -> PathBuf {
    let isbn = isbn.map_or(String::new(), |i| {
        format!(r#"<dc:identifier opf:scheme="ISBN">{i}</dc:identifier>"#)
    });
    let opf = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>{title}</dc:title>
    <dc:creator opf:role="aut">{creator}</dc:creator>
    <dc:identifier id="uid">urn:uuid:{salt}</dc:identifier>
    {isbn}
    <meta name="cover" content="cv"/>
  </metadata>
  <manifest><item id="cv" href="cover.png" media-type="image/png"/></manifest>
  <spine/>
</package>"#
    );
    let path = dir.join(name);
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    zip.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    zip.start_file("content.opf", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(opf.as_bytes()).unwrap();
    zip.start_file("cover.png", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&png(800, 1200)).unwrap();
    zip.finish().unwrap();
    path
}

fn added(outcome: ImportOutcome) -> (erc_library::Book, Option<i64>) {
    match outcome {
        ImportOutcome::Added {
            book,
            possible_duplicate_of,
        } => (book, possible_duplicate_of),
        other @ ImportOutcome::AlreadyInLibrary { .. } => panic!("expected Added, got {other:?}"),
    }
}

#[test]
fn imports_with_normalised_metadata_and_thumbnail() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let src = epub(
        dir.path(),
        "pl.epub",
        "Paradise Lost",
        "Milton, John; Pullman, Philip;",
        Some("9780192806192"),
        "a",
    );

    let (book, dup) = added(lib.import(&src).unwrap());
    assert_eq!(book.title, "Paradise Lost");
    assert_eq!(book.authors, ["John Milton", "Philip Pullman"]);
    assert!(dup.is_none());
    assert!(book.has_cover);
    let thumb = image::open(lib.cover_file(book.id).unwrap()).unwrap();
    assert_eq!(thumb.height(), 600);
    // The stored original is byte-identical to the source.
    assert_eq!(
        fs::read(lib.book_file(book.id)).unwrap(),
        fs::read(&src).unwrap()
    );
}

#[test]
fn detects_exact_and_likely_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let a = epub(dir.path(), "a.epub", "Dune", "Frank Herbert", None, "a");
    let (first, _) = added(lib.import(&a).unwrap());

    // Same file again.
    match lib.import(&a).unwrap() {
        ImportOutcome::AlreadyInLibrary { book } => assert_eq!(book.id, first.id),
        other @ ImportOutcome::Added { .. } => panic!("{other:?}"),
    }
    // Different file, same work (punctuation/case differences).
    let b = epub(dir.path(), "b.epub", "DUNE.", "Herbert, Frank", None, "b");
    let (_, dup) = added(lib.import(&b).unwrap());
    assert_eq!(dup, Some(first.id));
    // Different work.
    let c = epub(
        dir.path(),
        "c.epub",
        "Dune Messiah",
        "Frank Herbert",
        None,
        "c",
    );
    assert!(added(lib.import(&c).unwrap()).1.is_none());
    assert_eq!(lib.books().unwrap().len(), 3);
}

#[test]
fn edits_are_exported_but_originals_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let src = epub(dir.path(), "a.epub", "Old Title", "Someone", None, "a");
    let (book, _) = added(lib.import(&src).unwrap());

    let updated = lib
        .update(
            book.id,
            &MetadataEdit {
                title: "  New Title ".into(),
                authors: vec!["Ann Author".into(), " ".into()],
                series: Some("Saga".into()),
                series_index: Some("2".into()),
                tags: vec!["Sci-Fi".into()],
                contributors: None,
            },
        )
        .unwrap();
    assert_eq!(updated.title, "New Title");
    assert_eq!(updated.authors, ["Ann Author"]);

    let out = dir.path().join("export.epub");
    lib.export(book.id, &out).unwrap();
    let m = Epub::open(&out).unwrap().metadata();
    assert_eq!(m.title.as_deref(), Some("New Title"));
    assert_eq!(m.series.as_deref(), Some("Saga"));
    assert_eq!(m.tags, ["Sci-Fi"]);
    assert_eq!(
        fs::read(lib.book_file(book.id)).unwrap(),
        fs::read(&src).unwrap()
    );

    assert!(matches!(
        lib.update(
            book.id,
            &MetadataEdit {
                title: " ".into(),
                ..Default::default()
            }
        ),
        Err(Error::EmptyTitle)
    ));
}

#[test]
fn persists_and_removes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("lib");
    let id = {
        let mut lib = Library::open(&root).unwrap();
        let (book, _) = added(
            lib.import(&epub(
                dir.path(),
                "a.epub",
                "Emma",
                "Jane Austen",
                None,
                "a",
            ))
            .unwrap(),
        );
        lib.record_device_copy(
            book.id,
            "N123",
            "KEpub/Emma - Jane Austen.kepub.epub",
            "file:///mnt/onboard/KEpub/Emma - Jane Austen.kepub.epub",
        )
        .unwrap();
        book.id
    };
    let mut lib = Library::open(&root).unwrap();
    assert_eq!(lib.books().unwrap()[0].title, "Emma");
    assert_eq!(lib.device_copies("N123").unwrap()[0].book_id, id);

    lib.remove(id).unwrap();
    assert!(lib.books().unwrap().is_empty());
    assert!(lib.device_copies("N123").unwrap().is_empty());
    assert!(!lib.book_file(id).exists());
}

#[test]
fn failed_import_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let bad = dir.path().join("bad.epub");
    fs::write(&bad, b"not a zip").unwrap();
    assert!(lib.import(&bad).is_err());
    assert!(lib.books().unwrap().is_empty());
    assert_eq!(
        fs::read_dir(dir.path().join("lib/books")).unwrap().count(),
        0
    );
}

#[test]
fn custom_cover_is_used_on_export_and_can_be_reset() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let (book, _) = added(
        lib.import(&epub(
            dir.path(),
            "a.epub",
            "Dune",
            "Frank Herbert",
            None,
            "a",
        ))
        .unwrap(),
    );
    let original_thumb = fs::read(lib.cover_file(book.id).unwrap()).unwrap();

    // A tall JPEG gets capped to the Kobo screen height.
    let tall = {
        let img = image::RgbImage::from_pixel(1400, 2100, image::Rgb([10, 120, 200]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        out.into_inner()
    };
    let updated = lib.set_cover(book.id, &tall).unwrap();
    assert!(updated.custom_cover && updated.has_cover);
    assert_ne!(
        fs::read(lib.cover_file(book.id).unwrap()).unwrap(),
        original_thumb
    );

    let out = dir.path().join("out.epub");
    lib.export(book.id, &out).unwrap();
    let cover = Epub::open(&out).unwrap().cover().unwrap().unwrap();
    // The fixture's cover is a PNG, so the replacement is re-encoded to PNG
    // and swapped in place.
    assert_eq!(cover.media_type, "image/png");
    let img = image::load_from_memory(&cover.data).unwrap();
    assert_eq!((img.width(), img.height()), (1120, 1680));

    let reset = lib.reset_cover(book.id).unwrap();
    assert!(!reset.custom_cover && reset.has_cover);
    assert_eq!(
        fs::read(lib.cover_file(book.id).unwrap()).unwrap(),
        original_thumb
    );
    assert!(matches!(
        lib.set_cover(book.id, b"not an image"),
        Err(Error::BadImage(_))
    ));
}

#[test]
fn stores_rules() {
    use erc_library::{Rule, RuleKind};
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("lib");
    let saved = {
        let lib = Library::open(&root).unwrap();
        let rule = lib
            .save_rule(&Rule {
                id: 0,
                kind: RuleKind::Series,
                enabled: true,
                min_books: 0,
                prefix: "Series: ".into(),
            })
            .unwrap();
        assert_eq!(rule.min_books, 1, "at least one book");
        lib.save_rule(&Rule {
            enabled: false,
            ..rule.clone()
        })
        .unwrap()
    };
    let lib = Library::open(&root).unwrap();
    assert_eq!(lib.rules().unwrap(), std::slice::from_ref(&saved));
    assert!(!lib.rules().unwrap()[0].enabled);
    lib.delete_rule(saved.id).unwrap();
    assert!(lib.rules().unwrap().is_empty());
}

#[test]
fn contributors_are_stored_edited_exported_and_backfilled() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("lib");
    let mut lib = Library::open(&root).unwrap();
    let src = epub(dir.path(), "a.epub", "Title", "Ann Author", None, "c1");
    let (book, _) = added(lib.import(&src).unwrap());
    assert_eq!(book.contributors, Some(vec![]));

    let tom = Contributor {
        name: "Tom Translator".into(),
        role: "trl".into(),
    };
    let mut edit = book.edit();
    edit.contributors = Some(vec![
        Contributor {
            name: " Tom Translator ".into(),
            role: "TRL".into(),
        },
        Contributor {
            name: "  ".into(),
            role: "ill".into(),
        },
    ]);
    let updated = lib.update(book.id, &edit).unwrap();
    assert_eq!(updated.contributors, Some(vec![tom.clone()]));
    // An edit without contributors (e.g. from an older client) keeps them.
    let mut edit = updated.edit();
    edit.contributors = None;
    edit.title = "Retitled".into();
    assert_eq!(
        lib.update(book.id, &edit).unwrap().contributors,
        Some(vec![tom.clone()])
    );

    let out = dir.path().join("out.epub");
    lib.export(book.id, &out).unwrap();
    assert_eq!(
        Epub::open(&out).unwrap().metadata().contributors,
        std::slice::from_ref(&tom)
    );

    // A library from before contributors were tracked reads them on open.
    fs::copy(&out, lib.book_file(book.id)).unwrap();
    drop(lib);
    rusqlite::Connection::open(root.join("library.sqlite"))
        .unwrap()
        .execute("UPDATE books SET contributors = NULL", [])
        .unwrap();
    let lib = Library::open(&root).unwrap();
    assert_eq!(lib.book(book.id).unwrap().contributors, Some(vec![tom]));
}

#[test]
fn cover_quality_follows_the_kobo_screen_height() {
    use erc_library::{CUSTOM_COVER_MAX_HEIGHT, CoverQuality};
    assert_eq!(CoverQuality::of_height(1199), CoverQuality::Low);
    assert_eq!(CoverQuality::of_height(1200), CoverQuality::Decent);
    // The upscaler refuses from this height, so "excellent" must start here.
    assert_eq!(
        CoverQuality::of_height(CUSTOM_COVER_MAX_HEIGHT - 1),
        CoverQuality::Decent
    );
    assert_eq!(
        CoverQuality::of_height(CUSTOM_COVER_MAX_HEIGHT),
        CoverQuality::Excellent
    );
}

#[test]
fn upscaled_covers_are_remembered_until_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let (book, _) = added(
        lib.import(&epub(
            dir.path(),
            "a.epub",
            "Dune",
            "Frank Herbert",
            None,
            "a",
        ))
        .unwrap(),
    );
    let jpeg = |width, height| {
        let img = image::RgbImage::from_pixel(width, height, image::Rgb([10, 120, 200]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        out.into_inner()
    };

    // Before any change the source is the EPUB's own cover, not the thumbnail.
    let original = Epub::open(lib.book_file(book.id))
        .unwrap()
        .cover()
        .unwrap()
        .unwrap();
    assert_eq!(lib.cover_source(book.id).unwrap().unwrap(), original.data);

    let upscaled = lib.set_upscaled_cover(book.id, &jpeg(40, 60)).unwrap();
    assert!(upscaled.custom_cover && upscaled.cover_upscaled);
    assert_eq!(lib.book(book.id).unwrap(), upscaled);
    assert_eq!(lib.cover_size(book.id), Some((40, 60)));

    let replaced = lib.set_cover(book.id, &jpeg(20, 30)).unwrap();
    assert!(replaced.custom_cover && !replaced.cover_upscaled);
    let source = lib.cover_source(book.id).unwrap().unwrap();
    let source = image::load_from_memory(&source).unwrap();
    assert_eq!((source.width(), source.height()), (20, 30));

    lib.set_upscaled_cover(book.id, &jpeg(40, 60)).unwrap();
    let reset = lib.reset_cover(book.id).unwrap();
    assert!(!reset.custom_cover && !reset.cover_upscaled);
}

/// An EPUB with a publication date and a chapter of 600 words.
fn long_epub(dir: &Path) -> PathBuf {
    let opf = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Long</dc:title>
    <dc:identifier id="uid">urn:uuid:long</dc:identifier>
    <dc:date>1965-08-01</dc:date>
  </metadata>
  <manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;
    let path = dir.join("long.epub");
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    let options = SimpleFileOptions::default();
    for (name, data) in [
        (
            "META-INF/container.xml",
            r#"<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_owned(),
        ),
        ("content.opf", opf.to_owned()),
        ("c1.xhtml", format!("<body>{}</body>", "word ".repeat(600))),
    ] {
        zip.start_file(name, options).unwrap();
        zip.write_all(data.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    path
}

#[test]
fn length_and_publication_date_are_stored_and_backfilled() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("lib");
    let mut lib = Library::open(&root).unwrap();
    let (book, _) = added(lib.import(&long_epub(dir.path())).unwrap());
    assert_eq!(book.published.as_deref(), Some("1965-08-01"));
    assert_eq!(book.words, Some(600));
    assert_eq!(book.pages, Some(2));
    assert!(book.pages_estimated);

    // A library from before these were tracked reads them on open.
    drop(lib);
    rusqlite::Connection::open(root.join("library.sqlite"))
        .unwrap()
        .execute(
            "UPDATE books SET published = NULL, words = NULL, pages = NULL",
            [],
        )
        .unwrap();
    let lib = Library::open(&root).unwrap();
    let book = lib.book(book.id).unwrap();
    assert_eq!(book.published.as_deref(), Some("1965-08-01"));
    assert_eq!(book.words, Some(600));
    assert_eq!(book.pages, Some(2));
}

mod snapshots {
    use super::*;

    fn add(lib: &mut Library, dir: &Path, title: &str) -> i64 {
        let file = format!("{title}.epub");
        let path = epub(dir, &file, title, "An Author", None, title);
        added(lib.import(&path).unwrap()).0.id
    }

    fn titles(lib: &Library) -> Vec<String> {
        let mut titles: Vec<String> = lib.books().unwrap().into_iter().map(|b| b.title).collect();
        titles.sort();
        titles
    }

    #[test]
    fn going_back_brings_a_removed_book_back_with_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::open(dir.path().join("lib")).unwrap();
        let id = add(&mut lib, dir.path(), "Dune");
        let bytes = fs::read(lib.book_file(id)).unwrap();

        lib.snapshot("remove", Some("Dune")).unwrap();
        lib.remove(id).unwrap();
        assert!(lib.books().unwrap().is_empty());
        assert!(!lib.book_file(id).exists());

        let snapshot = lib.snapshots()[0].clone();
        assert_eq!(snapshot.op, "remove");
        assert_eq!(snapshot.detail.as_deref(), Some("Dune"));
        assert_eq!(snapshot.books, 1);

        let outcome = lib.restore_snapshot(&snapshot.file).unwrap();
        assert_eq!(outcome.restored, 1);
        assert_eq!(titles(&lib), ["Dune"]);
        assert_eq!(fs::read(lib.book_file(id)).unwrap(), bytes);
    }

    #[test]
    fn going_back_undoes_edits_and_can_itself_be_undone() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::open(dir.path().join("lib")).unwrap();
        let id = add(&mut lib, dir.path(), "Dune");
        let snapshot = lib.snapshot("tidy", None).unwrap();
        let file = snapshot.file_name().unwrap().to_string_lossy().into_owned();

        let mut edit = lib.book(id).unwrap().edit();
        edit.title = "Edited".into();
        lib.update(id, &edit).unwrap();

        let outcome = lib.restore_snapshot(&file).unwrap();
        assert_eq!(titles(&lib), ["Dune"]);
        lib.restore_snapshot(&outcome.safety_snapshot).unwrap();
        assert_eq!(titles(&lib), ["Edited"]);
    }

    #[test]
    fn books_added_since_are_set_aside_and_can_come_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::open(dir.path().join("lib")).unwrap();
        add(&mut lib, dir.path(), "Dune");
        let file = lib.snapshot("calibre", None).unwrap();
        let file = file.file_name().unwrap().to_string_lossy().into_owned();
        let later = add(&mut lib, dir.path(), "Hyperion");

        let outcome = lib.restore_snapshot(&file).unwrap();
        assert_eq!(outcome.set_aside, 1);
        assert_eq!(titles(&lib), ["Dune"]);
        assert!(!lib.book_file(later).exists());

        let undone = lib.restore_snapshot(&outcome.safety_snapshot).unwrap();
        assert_eq!(undone.restored, 1);
        assert_eq!(titles(&lib), ["Dune", "Hyperion"]);
        assert!(lib.book_file(later).exists());
    }

    #[test]
    fn a_reused_id_gets_each_book_its_own_files() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::open(dir.path().join("lib")).unwrap();
        let id = add(&mut lib, dir.path(), "Dune");
        let bytes = fs::read(lib.book_file(id)).unwrap();
        let file = lib.snapshot("remove", None).unwrap();
        let file = file.file_name().unwrap().to_string_lossy().into_owned();
        lib.remove(id).unwrap();
        let reused = add(&mut lib, dir.path(), "A much longer title than the first");
        assert_eq!(reused, id);

        lib.restore_snapshot(&file).unwrap();
        assert_eq!(titles(&lib), ["Dune"]);
        assert_eq!(fs::read(lib.book_file(id)).unwrap(), bytes);
    }

    #[test]
    fn settings_survive_going_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::open(dir.path().join("lib")).unwrap();
        let file = lib.snapshot("tidy", None).unwrap();
        let file = file.file_name().unwrap().to_string_lossy().into_owned();
        lib.set_setting("key", Some("value")).unwrap();

        lib.restore_snapshot(&file).unwrap();
        assert_eq!(lib.setting("key").unwrap().as_deref(), Some("value"));
    }

    #[test]
    fn a_book_whose_files_are_gone_is_reported_not_left_broken() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lib");
        let mut lib = Library::open(&root).unwrap();
        let id = add(&mut lib, dir.path(), "Dune");
        let file = lib.snapshot("remove", None).unwrap();
        let file = file.file_name().unwrap().to_string_lossy().into_owned();
        lib.remove(id).unwrap();
        fs::remove_dir_all(root.join("removed")).unwrap();

        let outcome = lib.restore_snapshot(&file).unwrap();
        assert_eq!(outcome.lost, ["Dune"]);
        assert!(lib.books().unwrap().is_empty());
    }

    #[test]
    fn refuses_damaged_or_unknown_snapshots_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("lib");
        let mut lib = Library::open(&root).unwrap();
        add(&mut lib, dir.path(), "Dune");
        let path = lib.snapshot("tidy", None).unwrap();
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        fs::write(&path, b"not a database").unwrap();

        assert!(lib.snapshots()[0].blocked.is_some());
        for name in [file.as_str(), "library-none.sqlite", "../library.sqlite"] {
            assert!(matches!(
                lib.restore_snapshot(name),
                Err(Error::CannotRestore(_))
            ));
        }
        assert_eq!(titles(&lib), ["Dune"]);
    }

    #[test]
    fn keeps_a_limited_number_of_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path().join("lib")).unwrap();
        for _ in 0..erc_library::SNAPSHOTS_KEPT + 3 {
            lib.snapshot("tidy", None).unwrap();
        }
        assert_eq!(lib.snapshots().len(), erc_library::SNAPSHOTS_KEPT);
    }
}
