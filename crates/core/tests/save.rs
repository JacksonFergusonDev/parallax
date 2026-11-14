use std::fs;
use std::path::Path;

use parallax_core::save::{Compression, DEFAULT_TEMPLATE, Destination, save_books};
use parallax_library::{ImportOutcome, Library};

fn library_with_two() -> (tempfile::TempDir, Library, Vec<i64>) {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/epub");
    let mut ids = Vec::new();
    for name in ["minimal.epub", "minimal.epub"] {
        // The same file twice would be a duplicate, so give the second its
        // own bytes.
        let copy = dir.path().join(format!("{}-{name}", ids.len()));
        let mut bytes = fs::read(fixture.join(name)).unwrap();
        bytes.extend_from_slice(&[0; 1][..ids.len()]);
        fs::write(&copy, bytes).unwrap();
        let ImportOutcome::Added { book, .. } = library.import(&copy).unwrap() else {
            panic!();
        };
        ids.push(book.id);
    }
    (dir, library, ids)
}

#[test]
fn saves_books_into_folders_without_overwriting() {
    let (dir, library, ids) = library_with_two();
    let out = dir.path().join("out");
    let saved = save_books(
        &library,
        &ids,
        DEFAULT_TEMPLATE,
        &Destination::Folder(out.clone()),
        |_, _| {},
    )
    .unwrap();
    let paths: Vec<String> = saved.iter().map(|s| s.result.clone().unwrap()).collect();
    assert_eq!(paths.len(), 2);
    assert_ne!(paths[0], paths[1], "same title, so the second is numbered");
    assert!(paths[1].ends_with(" (2).epub"), "{}", paths[1]);
    for path in &paths {
        assert!(Path::new(path).starts_with(&out));
        parallax_epub::Epub::open(path).unwrap();
    }
}

#[test]
fn saves_books_into_one_archive() {
    let (dir, library, ids) = library_with_two();
    let archive = dir.path().join("Books.zip");
    for compression in [Compression::Store, Compression::Normal] {
        let saved = save_books(
            &library,
            &ids,
            "{title}",
            &Destination::Archive(archive.clone(), compression),
            |_, _| {},
        )
        .unwrap();
        assert!(saved.iter().all(|s| s.result.is_ok()));
        let mut zip = zip::ZipArchive::new(fs::File::open(&archive).unwrap()).unwrap();
        assert_eq!(zip.len(), 2);
        let entry = zip.by_index(0).unwrap();
        assert_eq!(
            entry.compression(),
            if compression == Compression::Store {
                zip::CompressionMethod::Stored
            } else {
                zip::CompressionMethod::Deflated
            }
        );
        assert!(entry.name().unwrap().ends_with(".epub"));
    }
    assert!(!dir.path().join("Books.zip.partial").exists());
}
