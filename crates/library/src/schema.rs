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
];

pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "foreign_keys", true)?;
    conn.pragma_update(None, "journal_mode", "wal")?;
    let current: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}
