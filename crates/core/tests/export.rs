//! Exporting books to an archive and importing that archive into another
//! library brings back what the EPUB can't carry.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use parallax_convert::amazon::InProcess;
use parallax_convert::{Converter, Kepubify};
use parallax_core::export::{
    BookFormat, DEFAULT_TEMPLATE, Destination, ExportOptions, MANIFEST, export_books, read_manifest,
};
use parallax_core::ingest::import_archive;
use parallax_epub::{Identifier, MetadataEdit};
use parallax_library::{Book, ImportOutcome, Library};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/epub/minimal.epub")
}

/// A library with one book, given a series, a genre and enhanced details.
fn library(dir: &Path) -> (Library, Book) {
    let mut library = Library::open(dir).unwrap();
    let ImportOutcome::Added { book, .. } = library.import(&fixture()).unwrap() else {
        panic!("expected a new book");
    };
    library
        .update(
            book.id,
            &MetadataEdit {
                title: "Leviathan Wakes".into(),
                authors: vec!["James S. A. Corey".into()],
                contributors: None,
                series: Some("The Expanse".into()),
                series_index: Some("1".into()),
                tags: vec!["sci-fi".into()],
            },
        )
        .unwrap();
    let book = library
        .fill_details(
            book.id,
            &[Identifier {
                scheme: Some("goodreads".into()),
                value: "8855321".into(),
            }],
            Some("Orbit"),
            Some("The first book of the Expanse."),
            Some("en"),
        )
        .unwrap();
    (library, book)
}

fn entry_names(path: &Path) -> Vec<String> {
    let zip = zip::ZipArchive::new(File::open(path).unwrap()).unwrap();
    zip.file_names().map(|n| n.unwrap().into_owned()).collect()
}

fn export(
    library: &Library,
    id: i64,
    options: &ExportOptions,
    to: &Path,
    kepubify: Option<&dyn Converter>,
) {
    let exported = export_books(
        library,
        &[id],
        options,
        &Destination::Archive(to.to_path_buf()),
        kepubify,
        |_, _| {},
    )
    .unwrap();
    assert!(exported.iter().all(|e| e.result.is_ok()), "{exported:?}");
}

#[test]
fn an_archive_brings_back_details_the_epub_cannot_carry() {
    let dir = tempfile::tempdir().unwrap();
    let (source, book) = library(&dir.path().join("source"));
    let archive = dir.path().join("Library.zip");
    let settings = BTreeMap::from([("theme".to_owned(), "dark".to_owned())]);
    export(
        &source,
        book.id,
        &ExportOptions {
            template: DEFAULT_TEMPLATE.into(),
            format: BookFormat::Epub,
            settings: Some(settings.clone()),
        },
        &archive,
        None,
    );

    let mut names = entry_names(&archive);
    names.sort();
    assert_eq!(
        names,
        [
            "James S. A. Corey/The Expanse 1 - Leviathan Wakes.epub",
            MANIFEST
        ]
    );
    let manifest = read_manifest(&archive).unwrap();
    assert_eq!(manifest.settings, Some(settings));
    assert!(!dir.path().join("Library.zip.partial").exists());

    let mut target = Library::open(dir.path().join("target")).unwrap();
    let results = import_archive(&mut target, &InProcess, &archive, |_| {});
    let [(_, Ok(ImportOutcome::Added { book: imported, .. }))] = results.as_slice() else {
        panic!("expected one new book: {results:?}");
    };
    assert_eq!(imported.title, "Leviathan Wakes");
    assert_eq!(imported.series.as_deref(), Some("The Expanse"));
    assert_eq!(imported.tags, ["sci-fi"]);
    assert_eq!(imported.publisher, book.publisher);
    assert_eq!(imported.description, book.description);
    assert_eq!(imported.language, book.language);
    assert!(
        imported
            .identifiers
            .iter()
            .any(|i| i.scheme.as_deref() == Some("goodreads") && i.value == "8855321"),
        "{:?}",
        imported.identifiers
    );
}

#[test]
fn settings_are_left_out_unless_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let (source, book) = library(&dir.path().join("source"));
    let archive = dir.path().join("Library.zip");
    export(
        &source,
        book.id,
        &ExportOptions {
            template: DEFAULT_TEMPLATE.into(),
            ..ExportOptions::default()
        },
        &archive,
        None,
    );
    let manifest = read_manifest(&archive).unwrap();
    assert_eq!(manifest.settings, None);
    assert_eq!(manifest.books.len(), 1);
}

#[test]
fn a_folder_export_has_no_manifest() {
    let dir = tempfile::tempdir().unwrap();
    let (source, book) = library(&dir.path().join("source"));
    let folder = dir.path().join("Books");
    let exported = export_books(
        &source,
        &[book.id, book.id],
        &ExportOptions {
            template: "{genre}/{title}".into(),
            ..ExportOptions::default()
        },
        &Destination::Folder(folder.clone()),
        None,
        |_, _| {},
    )
    .unwrap();
    assert_eq!(exported.len(), 2);
    assert!(
        folder
            .join("Science Fiction/Leviathan Wakes.epub")
            .is_file()
    );
    assert!(
        folder
            .join("Science Fiction/Leviathan Wakes (2).epub")
            .is_file()
    );
    assert!(!folder.join(MANIFEST).exists());
}

#[test]
fn a_kepub_export_imports_back_as_a_plain_epub() {
    let dir = tempfile::tempdir().unwrap();
    let (source, book) = library(&dir.path().join("source"));
    let archive = dir.path().join("Kobo.zip");
    let kepubify = Kepubify::locate().unwrap();
    export(
        &source,
        book.id,
        &ExportOptions {
            template: "{title}".into(),
            format: BookFormat::Kepub,
            settings: None,
        },
        &archive,
        Some(&kepubify),
    );
    assert!(entry_names(&archive).contains(&"Leviathan Wakes.kepub.epub".to_owned()));

    let mut target = Library::open(dir.path().join("target")).unwrap();
    let results = import_archive(&mut target, &InProcess, &archive, |_| {});
    let [(_, Ok(ImportOutcome::Added { book: imported, .. }))] = results.as_slice() else {
        panic!("expected one new book: {results:?}");
    };
    assert_eq!(imported.publisher.as_deref(), Some("Orbit"));
    let mut zip = zip::ZipArchive::new(File::open(target.book_file(imported.id)).unwrap()).unwrap();
    for i in 0..zip.len() {
        let mut text = String::new();
        let mut entry = zip.by_index(i).unwrap();
        if entry.name().unwrap().ends_with("html") {
            entry.read_to_string(&mut text).unwrap();
            assert!(!text.contains("koboSpan"), "{}", entry.name().unwrap());
        }
    }
}

#[test]
fn kepub_needs_a_converter() {
    let dir = tempfile::tempdir().unwrap();
    let (source, book) = library(&dir.path().join("source"));
    let error = export_books(
        &source,
        &[book.id],
        &ExportOptions {
            template: DEFAULT_TEMPLATE.into(),
            format: BookFormat::Kepub,
            settings: None,
        },
        &Destination::Archive(dir.path().join("x.zip")),
        None,
        |_, _| {},
    )
    .unwrap_err();
    assert!(error.contains("KEPUB"));
}

/// Two books with the same title, from slightly different files.
fn library_with_two(dir: &Path) -> (Library, Vec<i64>) {
    let mut library = Library::open(dir.join("lib")).unwrap();
    let mut ids = Vec::new();
    for extra in [0usize, 1] {
        let copy = dir.join(format!("{extra}-minimal.epub"));
        let mut bytes = std::fs::read(fixture()).unwrap();
        bytes.extend_from_slice(&[0; 1][..extra]);
        std::fs::write(&copy, bytes).unwrap();
        let ImportOutcome::Added { book, .. } = library.import(&copy).unwrap() else {
            panic!("expected a new book");
        };
        ids.push(book.id);
    }
    (library, ids)
}

#[test]
fn exports_books_into_folders_without_overwriting() {
    let dir = tempfile::tempdir().unwrap();
    let (library, ids) = library_with_two(dir.path());
    let out = dir.path().join("out");
    let exported = export_books(
        &library,
        &ids,
        &ExportOptions {
            template: DEFAULT_TEMPLATE.into(),
            ..ExportOptions::default()
        },
        &Destination::Folder(out.clone()),
        None,
        |_, _| {},
    )
    .unwrap();
    let paths: Vec<String> = exported.iter().map(|e| e.result.clone().unwrap()).collect();
    assert_ne!(paths[0], paths[1], "same title, so the second is numbered");
    assert!(paths[1].ends_with(" (2).epub"), "{}", paths[1]);
    for path in &paths {
        assert!(Path::new(path).starts_with(&out));
        parallax_epub::Epub::open(path).unwrap();
    }
}

#[test]
fn exports_books_into_one_compressed_archive() {
    let dir = tempfile::tempdir().unwrap();
    let (library, ids) = library_with_two(dir.path());
    let archive = dir.path().join("Books.zip");
    let exported = export_books(
        &library,
        &ids,
        &ExportOptions {
            template: "{title}".into(),
            ..ExportOptions::default()
        },
        &Destination::Archive(archive.clone()),
        None,
        |_, _| {},
    )
    .unwrap();
    assert!(exported.iter().all(|e| e.result.is_ok()), "{exported:?}");
    let mut zip = zip::ZipArchive::new(File::open(&archive).unwrap()).unwrap();
    assert_eq!(zip.len(), 3, "two books and the manifest");
    for i in 0..zip.len() {
        assert_eq!(
            zip.by_index(i).unwrap().compression(),
            zip::CompressionMethod::Deflated
        );
    }
    assert_eq!(read_manifest(&archive).unwrap().books.len(), 2);
}
