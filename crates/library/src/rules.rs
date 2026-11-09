//! Automatic grouping rules, stored with the library. Planning and applying
//! them against a device lives in `parallax-core`.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::{Error, Library, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    /// One collection per series.
    Series,
    /// One collection per tag.
    Tag,
    /// One collection per author.
    Author,
    /// One collection per genre (tags that are genres, under their standard
    /// names, or the genre folder a book sits in on the Kobo).
    Genre,
}

impl RuleKind {
    fn as_str(self) -> &'static str {
        match self {
            RuleKind::Series => "series",
            RuleKind::Tag => "tag",
            RuleKind::Author => "author",
            RuleKind::Genre => "genre",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "series" => Some(RuleKind::Series),
            "tag" => Some(RuleKind::Tag),
            "author" => Some(RuleKind::Author),
            "genre" => Some(RuleKind::Genre),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    /// 0 for a rule not saved yet.
    pub id: i64,
    pub kind: RuleKind,
    pub enabled: bool,
    /// Only make a collection once at least this many books share the value.
    pub min_books: usize,
    /// Prepended to collection names, e.g. "Series: ".
    pub prefix: String,
}

impl Library {
    pub fn rules(&self) -> Result<Vec<Rule>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, kind, enabled, min_books, prefix FROM rules ORDER BY id")?;
        let rules = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get(2)?,
                    r.get::<_, i64>(3)?,
                    r.get(4)?,
                ))
            })?
            .filter_map(|row| {
                let (id, kind, enabled, min_books, prefix) = row.ok()?;
                Some(Rule {
                    id,
                    kind: RuleKind::parse(&kind)?,
                    enabled,
                    min_books: usize::try_from(min_books).unwrap_or(1).max(1),
                    prefix,
                })
            })
            .collect();
        Ok(rules)
    }

    /// Inserts (`id == 0`) or updates a rule; returns it with its id.
    pub fn save_rule(&self, rule: &Rule) -> Result<Rule> {
        let min_books = rule.min_books.max(1);
        let id = if rule.id == 0 {
            self.conn.execute(
                "INSERT INTO rules (kind, enabled, min_books, prefix) VALUES (?1, ?2, ?3, ?4)",
                params![rule.kind.as_str(), rule.enabled, min_books, rule.prefix],
            )?;
            self.conn.last_insert_rowid()
        } else {
            let changed = self.conn.execute(
                "UPDATE rules SET kind = ?2, enabled = ?3, min_books = ?4, prefix = ?5 WHERE id = ?1",
                params![rule.id, rule.kind.as_str(), rule.enabled, min_books, rule.prefix],
            )?;
            if changed == 0 {
                return Err(Error::NotFound(rule.id));
            }
            rule.id
        };
        Ok(Rule {
            id,
            min_books,
            ..rule.clone()
        })
    }

    pub fn delete_rule(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM rules WHERE id = ?1", [id])?;
        Ok(())
    }
}
