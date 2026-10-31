//! Native Kobo collections (`Shelf` / `ShelfContent`), ported from calibre's
//! KoboTouch `check_for_bookshelf`, `set_bookshelf`, `remove_from_bookshelves`
//! and `delete_empty_bookshelves`. Kobo stores booleans as the strings
//! `'true'`/`'false'` (only Tolino uses real booleans).
//!
//! Membership is keyed by `ContentID`, which for sideloaded books is the
//! predictable `file:///mnt/onboard/<path>`, so rows can be written before
//! Nickel imports a book (verified on firmware 4.38, spike S1).

use chrono::Utc;
use rusqlite::{OptionalExtension, Transaction, params};

use crate::error::{Error, Result};

/// Shelves Nickel manages itself; never renamed, emptied or deleted.
const RESERVED: [&str; 2] = ["Shortlist", "Wishlist"];

fn now() -> String {
    // calibre KOBOTOUCH.TIMESTAMP_STRING
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn check_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::InvalidCollection(
            "collection names can't be empty".into(),
        ));
    }
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name)) {
        return Err(Error::InvalidCollection(format!(
            "“{name}” is managed by the Kobo"
        )));
    }
    Ok(name)
}

/// Creates the collection, or undeletes it if it was soft-deleted.
pub fn create_collection(tx: &Transaction, name: &str) -> Result<()> {
    let name = check_name(name)?;
    let existing: Option<String> = tx
        .query_row(
            "SELECT _IsDeleted FROM Shelf WHERE Name = ?1",
            [name],
            |r| r.get(0),
        )
        .optional()?;
    match existing.as_deref() {
        None => {
            let ts = now();
            tx.execute(
                "INSERT INTO Shelf (CreationDate, InternalName, LastModified, Name, _IsDeleted, _IsVisible, _IsSynced, Id, Type)
                 VALUES (?1, ?2, ?1, ?2, 'false', 'true', 'false', ?2, 'UserTag')",
                params![ts, name],
            )?;
        }
        Some("true") => {
            tx.execute(
                "UPDATE Shelf SET _IsDeleted = 'false', LastModified = ?2 WHERE Name = ?1",
                params![name, now()],
            )?;
        }
        Some(_) => {}
    }
    Ok(())
}

/// Adds books to a collection, creating it if needed. Returns how many
/// memberships were added (books already in it are skipped).
pub fn add_to_collection(tx: &Transaction, name: &str, content_ids: &[String]) -> Result<usize> {
    create_collection(tx, name)?;
    let name = name.trim();
    let mut added = 0;
    for id in content_ids {
        let existing: Option<String> = tx
            .query_row(
                "SELECT _IsDeleted FROM ShelfContent WHERE ShelfName = ?1 AND ContentId = ?2",
                [name, id],
                |r| r.get(0),
            )
            .optional()?;
        match existing.as_deref() {
            None => {
                tx.execute(
                    "INSERT INTO ShelfContent (ShelfName, ContentId, DateModified, _IsDeleted, _IsSynced)
                     VALUES (?1, ?2, ?3, 'false', 'false')",
                    params![name, id, now()],
                )?;
                added += 1;
            }
            Some("true") => {
                tx.execute(
                    "UPDATE ShelfContent SET _IsDeleted = 'false', DateModified = ?3 WHERE ShelfName = ?1 AND ContentId = ?2",
                    params![name, id, now()],
                )?;
                added += 1;
            }
            Some(_) => {}
        }
    }
    if added > 0 {
        touch(tx, name)?;
    }
    Ok(added)
}

/// Removes books from a collection (the collection itself stays).
pub fn remove_from_collection(
    tx: &Transaction,
    name: &str,
    content_ids: &[String],
) -> Result<usize> {
    let mut removed = 0;
    for id in content_ids {
        removed += tx.execute(
            "DELETE FROM ShelfContent WHERE ShelfName = ?1 AND ContentId = ?2",
            [name, id],
        )?;
    }
    if removed > 0 {
        touch(tx, name)?;
    }
    Ok(removed)
}

/// Renames a collection, keeping its books. Fails if `new` already exists.
pub fn rename_collection(tx: &Transaction, old: &str, new: &str) -> Result<()> {
    check_name(old)?;
    let new = check_name(new)?;
    if old == new {
        return Ok(());
    }
    let clash: i64 = tx.query_row(
        "SELECT count(*) FROM Shelf WHERE Name = ?1 AND _IsDeleted != 'true'",
        [new],
        |r| r.get(0),
    )?;
    if clash > 0 {
        return Err(Error::InvalidCollection(format!(
            "a collection called “{new}” already exists"
        )));
    }
    // Clear out a soft-deleted shelf of the same name so the rename can take it.
    tx.execute(
        "DELETE FROM Shelf WHERE Name = ?1 AND _IsDeleted = 'true' AND _IsSynced != 'true'",
        [new],
    )?;
    let changed = tx.execute(
        "UPDATE Shelf SET Name = ?2, InternalName = ?2, LastModified = ?3, _IsSynced = 'false' WHERE Name = ?1",
        params![old, new, now()],
    )?;
    if changed == 0 {
        return Err(Error::InvalidCollection(format!(
            "no collection called “{old}”"
        )));
    }
    tx.execute(
        "UPDATE ShelfContent SET ShelfName = ?2, _IsSynced = 'false' WHERE ShelfName = ?1",
        [old, new],
    )?;
    Ok(())
}

/// Deletes a collection (not its books). Collections that have been synced
/// to the Kobo account are soft-deleted so the deletion syncs too.
pub fn delete_collection(tx: &Transaction, name: &str) -> Result<()> {
    let name = check_name(name)?;
    tx.execute("DELETE FROM ShelfContent WHERE ShelfName = ?1", [name])?;
    tx.execute(
        "DELETE FROM Shelf WHERE Name = ?1 AND _IsSynced != 'true' AND (Type IS NULL OR Type != 'SystemTag')",
        [name],
    )?;
    tx.execute(
        "UPDATE Shelf SET _IsDeleted = 'true', LastModified = ?2
         WHERE Name = ?1 AND _IsSynced = 'true' AND (Type IS NULL OR Type != 'SystemTag')",
        params![name, now()],
    )?;
    Ok(())
}

fn touch(tx: &Transaction, name: &str) -> Result<()> {
    tx.execute(
        "UPDATE Shelf SET LastModified = ?2 WHERE Name = ?1",
        params![name, now()],
    )?;
    Ok(())
}
