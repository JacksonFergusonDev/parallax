//! Library database schema, versioned with `PRAGMA user_version`.

use rusqlite::Connection;

/// Each entry upgrades the schema by one version. Never edit a released entry;
/// append a new one.
const MIGRATIONS: &[&str] = &[
    // 1: books and where they've been sent.
    "CREATE TABLE books (
        id            INTEGER PRIMARY KEY,
        title         TEXT NOT NULL,
        authors       TEXT NOT NULL DEFAULT '[]',
        series        TEXT,
        series_index  TEXT,
        tags          TEXT NOT NULL DEFAULT '[]',
        language      TEXT,
        publisher     TEXT,
        description   TEXT,
        identifiers   TEXT NOT NULL DEFAULT '[]',
        file_hash     TEXT NOT NULL UNIQUE,
        file_size     INTEGER NOT NULL,
        original_name TEXT NOT NULL,
        has_cover     INTEGER NOT NULL DEFAULT 0,
        added_at      TEXT NOT NULL,
        modified_at   TEXT NOT NULL
    );
    CREATE TABLE device_copies (
        book_id       INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
        device_serial TEXT NOT NULL,
        device_path   TEXT NOT NULL,
        content_id    TEXT NOT NULL,
        sent_at       TEXT NOT NULL,
        PRIMARY KEY (book_id, device_serial)
    );",
    // 2: a user-chosen cover in books/<id>/custom-cover.jpg replaces the EPUB's.
    "ALTER TABLE books ADD COLUMN custom_cover INTEGER NOT NULL DEFAULT 0;",
    // 3: automatic grouping rules (series/tag/author → collection).
    "CREATE TABLE rules (
        id        INTEGER PRIMARY KEY,
        kind      TEXT NOT NULL,
        enabled   INTEGER NOT NULL DEFAULT 1,
        min_books INTEGER NOT NULL DEFAULT 2,
        prefix    TEXT NOT NULL DEFAULT ''
    );",
    // 4: duplicate groups the user said aren't duplicates. `members` is the
    // sorted member ids joined with newlines; scope is "library" or
    // "device:<serial>".
    "CREATE TABLE dismissed_duplicates (
        scope   TEXT NOT NULL,
        members TEXT NOT NULL,
        PRIMARY KEY (scope, members)
    );",
    // 5: app settings, e.g. an optional Google Books API key.
    "CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );",
    // 6: reading progress last seen on each device.
    "CREATE TABLE reading (
        book_id       INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
        device_serial TEXT NOT NULL,
        status        TEXT NOT NULL,
        percent       INTEGER NOT NULL,
        last_read     TEXT,
        time_spent    INTEGER,
        seen_at       TEXT NOT NULL,
        PRIMARY KEY (book_id, device_serial)
    );",
    // 7: translators, illustrators… as JSON; NULL until read from the EPUB
    // (books imported earlier are filled in when the library opens).
    "ALTER TABLE books ADD COLUMN contributors TEXT;",
    // 8: highlights, notes and bookmarks copied from devices. Kept when
    // removed on the device (on_device = 0), so this is also a backup.
    "CREATE TABLE annotations (
        device_serial TEXT NOT NULL,
        bookmark_id   TEXT NOT NULL,
        book_id       INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
        kind          TEXT NOT NULL,
        text          TEXT,
        note          TEXT,
        chapter       TEXT,
        position      REAL NOT NULL,
        created       TEXT,
        modified      TEXT,
        on_device     INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (device_serial, bookmark_id)
    );",
    // 9: books imported from a calibre library, by calibre's book uuid, so
    // importing again matches them even after edits.
    "CREATE TABLE calibre_links (
        calibre_uuid TEXT PRIMARY KEY,
        book_id      INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE
    );",
    // 10: the latest reading statistics read from each device (JSON), so they
    // can be shown while it's unplugged.
    "CREATE TABLE reading_stats (
        device_serial TEXT PRIMARY KEY,
        taken_at      TEXT NOT NULL,
        stats         TEXT NOT NULL
    );",
    // 11: the custom cover was enlarged by the upscaler rather than found.
    "ALTER TABLE books ADD COLUMN cover_upscaled INTEGER NOT NULL DEFAULT 0;",
    // 12: publication date and length; `words` is NULL until the EPUB has been
    // measured (books imported earlier are filled in when the library opens).
    "ALTER TABLE books ADD COLUMN published TEXT;
     ALTER TABLE books ADD COLUMN words INTEGER;
     ALTER TABLE books ADD COLUMN pages INTEGER;
     ALTER TABLE books ADD COLUMN pages_estimated INTEGER NOT NULL DEFAULT 1;",
    // 13: where the in-app reader was last in each book.
    "CREATE TABLE reader_positions (
        book_id    INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
        href       TEXT NOT NULL,
        cfi        TEXT NOT NULL,
        fraction   REAL NOT NULL,
        quote      TEXT,
        updated_at TEXT NOT NULL
    );",
    // 14: highlights and notes made in the in-app reader. Kept apart from
    // `annotations`, which mirrors what devices report.
    "CREATE TABLE reader_highlights (
        id         INTEGER PRIMARY KEY,
        book_id    INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
        cfi        TEXT NOT NULL,
        href       TEXT NOT NULL,
        fraction   REAL NOT NULL,
        text       TEXT NOT NULL,
        note       TEXT,
        color      TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
    CREATE INDEX reader_highlights_book ON reader_highlights(book_id, fraction);",
    // 15: comics. `format` is the stored file (`epub`, or `cbz` in
    // books/<id>/book.cbz), `kind` is book, comic or manga, and `direction`
    // is which way pages turn (`ltr` or `rtl`).
    "ALTER TABLE books ADD COLUMN format TEXT NOT NULL DEFAULT 'epub';
     ALTER TABLE books ADD COLUMN kind TEXT NOT NULL DEFAULT 'book';
     ALTER TABLE books ADD COLUMN direction TEXT NOT NULL DEFAULT 'ltr';",
    // 16: the effective comic preparation profile of each device copy.
    "ALTER TABLE device_copies ADD COLUMN comic_profile TEXT;",
    // 17: books from Goodreads library exports, by Goodreads' Book Id.
    // `data` is the book as you see it, as JSON. `base` is the book as the
    // last export had it, so a newer export can be merged with edits made
    // here. `local_only` is set when you chose to keep a book a newer export
    // no longer had.
    "CREATE TABLE goodreads_books (
        goodreads_id      TEXT PRIMARY KEY,
        data              TEXT NOT NULL,
        base              TEXT NOT NULL,
        local_only        INTEGER NOT NULL DEFAULT 0,
        first_imported_at TEXT NOT NULL,
        updated_at        TEXT NOT NULL
    );",
];

/// The schema version this build creates.
pub fn latest() -> usize {
    MIGRATIONS.len()
}

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "journal_mode", "wal")?;
    let current: usize = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i + 1)?;
        tx.commit()?;
    }
    Ok(())
}
