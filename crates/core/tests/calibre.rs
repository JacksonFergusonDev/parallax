//! Importing a synthetic calibre library built from calibre's own table
//! definitions (tests/fixtures/calibre/schema.sql).

use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use erc_core::calibre::{CalibreImport, import, preview, read_library};
use erc_library::{ImportOutcome, Library};
use rusqlite::{Connection, params};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn jpeg(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(width, height, image::Rgb([30, 90, 160]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
    out.into_inner()
}

/// A small EPUB2; `kobo` adds the koboSpan markup a KEPUB has.
fn epub(path: &Path, title: &str, creator: &str, kobo: bool) {
    let opf = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>{title} (file title)</dc:title>
    <dc:creator opf:role="aut">{creator}</dc:creator>
    <dc:identifier id="uid">urn:uuid:{title}</dc:identifier>
  </metadata>
  <manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#
    );
    let body = if kobo {
        r#"<p><span class="koboSpan" id="kobo.1.1">Text.</span></p>"#
    } else {
        "<p>Text.</p>"
    };
    let xhtml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>{title}</title></head><body>{body}</body></html>"#
    );
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = ZipWriter::new(File::create(path).unwrap());
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
    zip.start_file("c1.xhtml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(xhtml.as_bytes()).unwrap();
    zip.finish().unwrap();
}

struct Calibre {
    root: PathBuf,
    conn: Connection,
}

impl Calibre {
    fn new(root: PathBuf) -> Self {
        fs::create_dir_all(&root).unwrap();
        let conn = Connection::open(root.join("metadata.db")).unwrap();
        conn.execute_batch(include_str!("../../../tests/fixtures/calibre/schema.sql"))
            .unwrap();
        Self { root, conn }
    }

    /// Adds a book with files in `formats` ("EPUB", "KEPUB", "PDF").
    fn book(&self, id: i64, title: &str, author: &str, formats: &[&str], cover: bool) {
        let path = format!("{author}/{title} ({id})");
        let name = format!("{title} - {author}");
        self.conn
            .execute(
                "INSERT INTO books (id, title, sort, series_index, path, uuid, has_cover)
                 VALUES (?1, ?2, ?2, 1.0, ?3, ?4, ?5)",
                params![id, title, path, format!("uuid-{id}"), cover],
            )
            .unwrap();
        self.conn
            .execute(
                "INSERT OR IGNORE INTO authors (name, sort) VALUES (?1, ?1)",
                [author],
            )
            .unwrap();
        self.conn
            .execute(
                "INSERT INTO books_authors_link (book, author) SELECT ?1, id FROM authors WHERE name = ?2",
                params![id, author],
            )
            .unwrap();
        let dir = self.root.join(&path);
        fs::create_dir_all(&dir).unwrap();
        for format in formats {
            let file = dir.join(format!("{name}.{}", format.to_lowercase()));
            match *format {
                "EPUB" => epub(&file, title, author, false),
                "KEPUB" => epub(&file, title, author, true),
                _ => fs::write(&file, b"%PDF-1.4").unwrap(),
            }
            self.conn
                .execute(
                    "INSERT INTO data (book, format, uncompressed_size, name) VALUES (?1, ?2, 1, ?3)",
                    params![id, format, name],
                )
                .unwrap();
        }
        if cover {
            fs::write(dir.join("cover.jpg"), jpeg(600, 900)).unwrap();
        }
    }
}

#[test]
fn imports_calibre_metadata_once_and_skips_what_it_cannot_read() {
    let dir = tempfile::tempdir().unwrap();
    let cal = Calibre::new(dir.path().join("Calibre Library"));
    cal.book(1, "Hyperion", "Dan Simmons", &["EPUB", "KEPUB"], true);
    cal.conn
        .execute_batch(
            "INSERT INTO series (id, name, sort) VALUES (1, 'Hyperion Cantos', 'Hyperion Cantos');
             INSERT INTO books_series_link (book, series) VALUES (1, 1);
             UPDATE books SET series_index = 2.0 WHERE id = 1;
             INSERT INTO tags (id, name) VALUES (1, 'Sci-Fi'), (2, 'Classics');
             INSERT INTO books_tags_link (book, tag) VALUES (1, 1), (1, 2);
             INSERT INTO identifiers (book, type, val) VALUES (1, 'isbn', '9780553283686'), (1, 'mobi-asin', 'B000FC1PJI');
             INSERT INTO publishers (id, name) VALUES (1, 'Bantam');
             INSERT INTO books_publishers_link (book, publisher) VALUES (1, 1);
             INSERT INTO languages (id, lang_code) VALUES (1, 'eng');
             INSERT INTO books_languages_link (book, lang_code) VALUES (1, 1);
             INSERT INTO comments (book, text) VALUES (1, '<p>Pilgrims.</p>');",
        )
        .unwrap();
    cal.book(
        2,
        "The Shadow of the Gods",
        "John Gwynne",
        &["KEPUB"],
        false,
    );
    cal.book(3, "A Scanned Book", "Some One", &["PDF"], false);
    cal.book(4, "Already Here", "Ann Author", &["EPUB"], false);
    // A format calibre lists whose file is gone counts as missing.
    cal.conn
        .execute(
            "INSERT INTO data (book, format, uncompressed_size, name) VALUES (3, 'AZW3', 1, 'gone')",
            [],
        )
        .unwrap();

    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let mine = dir.path().join("mine.epub");
    epub(&mine, "Different file", "Someone", false);
    let ImportOutcome::Added { book: mine, .. } = library.import(&mine).unwrap() else {
        panic!();
    };
    let mut edit = mine.edit();
    edit.title = "Already Here".into();
    edit.authors = vec!["Ann Author".into()];
    library.update(mine.id, &edit).unwrap();

    let books = read_library(&cal.root).unwrap();
    assert_eq!(books.len(), 4);
    let hyperion = books.iter().find(|b| b.id == 1).unwrap();
    assert_eq!(hyperion.series.as_deref(), Some("Hyperion Cantos"));
    assert_eq!(hyperion.series_index.as_deref(), Some("2"));
    assert_eq!(hyperion.formats.len(), 2);
    assert_eq!(books.iter().find(|b| b.id == 3).unwrap().formats.len(), 1);

    let plan = preview(&library, &books);
    assert_eq!((plan.total, plan.new, plan.already), (4, 2, 1));
    assert_eq!(plan.no_epub, [("PDF".to_owned(), 1)]);

    let mut progress = Vec::new();
    let outcomes = import(&mut library, &books, |i, n, _| progress.push((i, n)));
    assert_eq!(progress.last(), Some(&(4, 4)));
    let result = |id: i64| {
        outcomes
            .iter()
            .find(|o| o.calibre_id == id)
            .unwrap()
            .result
            .clone()
            .unwrap()
    };
    let CalibreImport::Added { book_id } = result(1) else {
        panic!("{:?}", result(1));
    };
    let book = library.book(book_id).unwrap();
    assert_eq!(book.title, "Hyperion");
    assert_eq!(book.authors, ["Dan Simmons"]);
    assert_eq!(book.series.as_deref(), Some("Hyperion Cantos"));
    assert_eq!(book.series_index.as_deref(), Some("2"));
    assert_eq!(book.tags, ["Classics", "Sci-Fi"]);
    assert_eq!(book.isbn(), Some("9780553283686"));
    assert_eq!(book.asin(), Some("B000FC1PJI"));
    assert_eq!(book.publisher.as_deref(), Some("Bantam"));
    assert_eq!(book.language.as_deref(), Some("eng"));
    assert_eq!(book.description.as_deref(), Some("<p>Pilgrims.</p>"));
    assert!(book.custom_cover);
    assert_eq!(library.cover_size(book_id), Some((600, 900)));

    let CalibreImport::Added { book_id: kepub } = result(2) else {
        panic!("{:?}", result(2));
    };
    let stored = fs::read(library.book_file(kepub)).unwrap();
    assert!(!String::from_utf8_lossy(&stored).contains("koboSpan"));
    assert_eq!(library.book(kepub).unwrap().title, "The Shadow of the Gods");
    assert!(matches!(result(3), CalibreImport::Skipped { reason } if reason == "no EPUB (PDF)"));
    assert_eq!(result(4), CalibreImport::Matched { book_id: mine.id });
    assert_eq!(library.book(mine.id).unwrap().title, "Already Here");

    // Again, after editing an imported title: nothing new.
    let mut edit = library.book(book_id).unwrap().edit();
    edit.title = "Hyperion (edited)".into();
    library.update(book_id, &edit).unwrap();
    let count = library.books().unwrap().len();
    let again = import(
        &mut library,
        &read_library(&cal.root).unwrap(),
        |_, _, _| {},
    );
    assert_eq!(library.books().unwrap().len(), count);
    assert!(again.iter().all(|o| matches!(
        o.result,
        Ok(CalibreImport::Matched { .. } | CalibreImport::Skipped { .. })
    )));
    assert_eq!(preview(&library, &read_library(&cal.root).unwrap()).new, 0);
    // calibre's library is untouched: no journal or lock files appeared.
    let mut names: Vec<String> = fs::read_dir(&cal.root)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert!(
        names.iter().all(|n| !n.starts_with("metadata.db-")),
        "{names:?}"
    );
}

#[test]
fn rejects_a_folder_that_is_not_a_calibre_library() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_library(dir.path()).is_err());
}
