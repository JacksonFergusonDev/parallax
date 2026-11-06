//! Highlights, notes and bookmarks from Nickel's `Bookmark` table (as calibre's
//! Kobo driver and Kobo Utilities read them), with chapter titles from the
//! book's table of contents, plus Adobe `.annot` files for plain EPUBs.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::KoboDevice;
use crate::db::{KoboDb, columns};
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    Highlight,
    /// A highlight with a note.
    Note,
    /// A dog-ear: a marked page with no text.
    Bookmark,
    /// Handwriting on stylus models.
    Markup,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Annotation {
    /// Nickel's `BookmarkID`.
    pub id: String,
    /// The book's ContentID.
    pub book: String,
    pub kind: AnnotationKind,
    /// The highlighted passage.
    pub text: Option<String>,
    pub note: Option<String>,
    /// Table-of-contents title of the chapter it's in.
    pub chapter: Option<String>,
    /// Reading order within the book: chapter number plus progress through
    /// the chapter.
    pub position: f64,
    pub created: Option<String>,
    pub modified: Option<String>,
}

/// The file part of a chapter ContentID: `…kepub.epub!!OEBPS/c1.xhtml` and
/// the table-of-contents form `…!!OEBPS/c1.xhtml-2` both give `OEBPS/c1.xhtml`.
fn chapter_file(content_id: &str) -> Option<&str> {
    let (_, file) = content_id.split_once("!!")?;
    let file = file.split('#').next().unwrap_or(file);
    Some(match file.rsplit_once('-') {
        Some((stem, n)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => stem,
        _ => file,
    })
}

/// A book's spine order by chapter file, and its table of contents as
/// (spine index, order, title).
type Contents = (HashMap<String, i64>, Vec<(i64, i64, String)>);

fn clean(s: Option<String>) -> Option<String> {
    s.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

impl KoboDb {
    /// Visible annotations, for one book (by ContentID) or all, in reading
    /// order.
    pub fn annotations(&self, book: Option<&str>) -> Result<Vec<Annotation>> {
        type Row = (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            f64,
            Option<String>,
            Option<String>,
            String,
        );
        let conn = self.connection();
        // Older schemas lack some of these columns; read NULL instead.
        let present = columns(conn, "Bookmark")?;
        if present.is_empty() {
            return Ok(Vec::new());
        }
        let col = |name: &str| {
            if present.contains(&name.to_ascii_lowercase()) {
                name.to_owned()
            } else {
                "NULL".to_owned()
            }
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT BookmarkID, VolumeID, ContentID, {text}, {note}, {progress},
                    {created}, {modified}, coalesce({kind}, '')
             FROM Bookmark
             WHERE coalesce({hidden}, 'false') NOT IN ('true', 1)
               AND (?1 IS NULL OR VolumeID = ?1)",
            text = col("Text"),
            note = col("Annotation"),
            progress = col("ChapterProgress"),
            created = col("DateCreated"),
            modified = col("DateModified"),
            kind = col("Type"),
            hidden = col("Hidden"),
        ))?;
        let rows: Vec<Row> = stmt
            .query_map([book], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get::<_, Option<f64>>(5)?.unwrap_or(0.0),
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;

        // Per book: spine order of each chapter file, and the table of
        // contents as (spine index, title).
        let mut books: HashMap<String, Contents> = HashMap::new();
        for volume in rows.iter().map(|r| r.1.clone()) {
            if books.contains_key(&volume) {
                continue;
            }
            let mut spine = HashMap::new();
            let mut stmt = conn.prepare(
                "SELECT ContentID, VolumeIndex FROM content WHERE BookID = ?1 AND ContentType = 9",
            )?;
            for row in stmt.query_map([&volume], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?))
            })? {
                let (id, index) = row?;
                if let (Some(file), Some(index)) = (chapter_file(&id), index) {
                    spine.insert(file.to_owned(), index);
                }
            }
            let mut toc = Vec::new();
            let mut stmt = conn.prepare(
                "SELECT ContentID, Title, coalesce(VolumeIndex, 0) FROM content
                 WHERE BookID = ?1 AND ContentType = 899",
            )?;
            for row in stmt.query_map([&volume], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })? {
                let (id, title, order) = row?;
                if let (Some(index), Some(title)) =
                    (chapter_file(&id).and_then(|f| spine.get(f)), clean(title))
                {
                    toc.push((*index, order, title));
                }
            }
            toc.sort();
            books.insert(volume, (spine, toc));
        }

        let mut out: Vec<Annotation> = rows
            .into_iter()
            .map(
                |(id, volume, content, text, note, progress, created, modified, kind)| {
                    let (spine, toc) = &books[&volume];
                    let index = chapter_file(&content).and_then(|f| spine.get(f)).copied();
                    let chapter = index.and_then(|i| {
                        toc.iter()
                            .rev()
                            .find(|(at, _, _)| *at <= i)
                            .map(|(_, _, title)| title.clone())
                    });
                    let (text, note) = (clean(text), clean(note));
                    let kind = match kind.as_str() {
                        "dogear" => AnnotationKind::Bookmark,
                        "markup" => AnnotationKind::Markup,
                        _ if note.is_some() => AnnotationKind::Note,
                        _ if text.is_none() => AnnotationKind::Bookmark,
                        _ => AnnotationKind::Highlight,
                    };
                    Annotation {
                        id,
                        book: volume,
                        kind,
                        text,
                        note,
                        chapter,
                        position: index.unwrap_or(0) as f64 + progress.clamp(0.0, 1.0),
                        created,
                        modified,
                    }
                },
            )
            .collect();
        out.sort_by(|a, b| {
            a.book
                .cmp(&b.book)
                .then(a.position.total_cmp(&b.position))
                .then(a.created.cmp(&b.created))
        });
        Ok(out)
    }
}

impl KoboDevice {
    /// Adobe annotation files (`Digital Editions/Annotations/<book path>.annot`)
    /// that hold highlights for plain EPUBs and PDFs, as (path relative to
    /// the Annotations folder, absolute path).
    pub fn annotation_files(&self) -> Vec<(String, PathBuf)> {
        let root = self.mount().join("Digital Editions").join("Annotations");
        let mut out = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with("._") {
                    continue;
                }
                match entry.file_type() {
                    Ok(t) if t.is_dir() => stack.push(path),
                    Ok(t)
                        if t.is_file()
                            && path
                                .extension()
                                .is_some_and(|e| e.eq_ignore_ascii_case("annot")) =>
                    {
                        let rel = path
                            .strip_prefix(&root)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .into_owned();
                        out.push((rel, path));
                    }
                    _ => {}
                }
            }
        }
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_files_from_content_ids() {
        let book = "/mnt/onboard/KEpub/A.kepub.epub";
        assert_eq!(
            chapter_file(&format!("{book}!!OEBPS/xhtml/c1.xhtml")),
            Some("OEBPS/xhtml/c1.xhtml")
        );
        assert_eq!(
            chapter_file(&format!("{book}!!OEBPS/xhtml/c1.xhtml-2")),
            Some("OEBPS/xhtml/c1.xhtml")
        );
        assert_eq!(
            chapter_file(&format!("{book}!!OEBPS/part-two.xhtml")),
            Some("OEBPS/part-two.xhtml")
        );
        assert_eq!(chapter_file("file:///mnt/onboard/x.epub"), None);
    }
}
