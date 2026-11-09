//! Natural order for page and chapter names: `p2` before `p10`, folders
//! compared one level at a time, case ignored.

use std::cmp::Ordering;

/// Compares two archive paths the way people number pages.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut left = a.split('/');
    let mut right = b.split('/');
    loop {
        match (left.next(), right.next()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => match component_cmp(x, y) {
                Ordering::Equal => {}
                other => return other,
            },
        }
    }
}

fn component_cmp(left: &str, right: &str) -> Ordering {
    let (mut lefts, mut rights) = (chunks(left), chunks(right));
    loop {
        match (lefts.next(), rights.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(l), Some(r)) => {
                let order = match (l, r) {
                    (Chunk::Digits(l), Chunk::Digits(r)) => digits_cmp(l, r),
                    (Chunk::Text(l), Chunk::Text(r)) => l.to_lowercase().cmp(&r.to_lowercase()),
                    // Numbers sort before words, as in Finder.
                    (Chunk::Digits(_), Chunk::Text(_)) => Ordering::Less,
                    (Chunk::Text(_), Chunk::Digits(_)) => Ordering::Greater,
                };
                if order != Ordering::Equal {
                    return order;
                }
            }
        }
    }
}

/// Numeric comparison of digit runs of any length; `007` and `7` tie here.
fn digits_cmp(a: &str, b: &str) -> Ordering {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

enum Chunk<'a> {
    Digits(&'a str),
    Text(&'a str),
}

fn chunks(s: &str) -> impl Iterator<Item = Chunk<'_>> {
    let mut rest = s;
    std::iter::from_fn(move || {
        let first = rest.chars().next()?;
        let digit = first.is_ascii_digit();
        let end = rest
            .find(|c: char| c.is_ascii_digit() != digit)
            .unwrap_or(rest.len());
        let (chunk, tail) = rest.split_at(end);
        rest = tail;
        Some(if digit {
            Chunk::Digits(chunk)
        } else {
            Chunk::Text(chunk)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = names.iter().map(|s| (*s).to_owned()).collect();
        v.sort_by(|a, b| natural_cmp(a, b));
        v
    }

    #[test]
    fn numbers_compare_by_value() {
        assert_eq!(
            sorted(&["p10.jpg", "p9.jpg", "p1.jpg", "p100.jpg"]),
            ["p1.jpg", "p9.jpg", "p10.jpg", "p100.jpg"]
        );
    }

    #[test]
    fn padded_and_unpadded_mix() {
        assert_eq!(
            sorted(&["Chapter 10", "Chapter 002", "Chapter 1"]),
            ["Chapter 1", "Chapter 002", "Chapter 10"]
        );
    }

    #[test]
    fn folders_compare_level_by_level() {
        assert_eq!(
            sorted(&[
                "ch10/p1.jpg",
                "ch2/p10.jpg",
                "ch2/p2.jpg",
                "ch2 extra/p1.jpg"
            ]),
            [
                "ch2/p2.jpg",
                "ch2/p10.jpg",
                "ch2 extra/p1.jpg",
                "ch10/p1.jpg"
            ]
        );
    }

    #[test]
    fn case_is_ignored_and_ties_are_stable() {
        assert_eq!(sorted(&["B.jpg", "a.jpg"]), ["a.jpg", "B.jpg"]);
        assert_eq!(natural_cmp("p07", "p7"), "p07".cmp("p7"));
        assert_ne!(natural_cmp("p07", "p7"), Ordering::Equal);
    }

    #[test]
    fn real_volume_names_keep_reading_order() {
        // From the Berserk volumes in the Kobo spike.
        assert_eq!(
            sorted(&[
                "Berserk - 001 (v01) - p006-007.jpg",
                "Berserk - 001 (v01) - p000x2.jpg",
                "Berserk - 001 (v01) - p000.jpg",
                "Berserk - 001 (v01) - p000x1.png",
                "Berserk - 001 (v01) - p005.jpg",
                "Berserk - 003 (v01) - p223.jpg",
            ]),
            [
                "Berserk - 001 (v01) - p000.jpg",
                "Berserk - 001 (v01) - p000x1.png",
                "Berserk - 001 (v01) - p000x2.jpg",
                "Berserk - 001 (v01) - p005.jpg",
                "Berserk - 001 (v01) - p006-007.jpg",
                "Berserk - 003 (v01) - p223.jpg",
            ]
        );
    }
}
