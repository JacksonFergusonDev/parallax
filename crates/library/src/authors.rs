//! Cleaning up author names as found in the wild, e.g. a single creator of
//! `"Milton, John; Pullman, Philip;"`.

/// Splits combined author strings on `;` and ` & `, flips `Last, First` to
/// `First Last`, trims, and drops empties and duplicates.
pub fn normalize_authors(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for entry in raw {
        for part in entry.split(';').flat_map(|p| p.split(" & ")) {
            let name = flip(part.trim());
            if !name.is_empty() && !out.iter().any(|a| a.eq_ignore_ascii_case(&name)) {
                out.push(name);
            }
        }
    }
    out
}

/// `Lynch, Scott` → `Scott Lynch`. Leaves names with no comma, several
/// commas, or a suffix after the comma (`King, Jr.`) as they are.
fn flip(name: &str) -> String {
    let collapsed = name.split_whitespace().collect::<Vec<_>>().join(" ");
    match collapsed.split_once(',') {
        Some((last, first)) if !first.contains(',') => {
            let (last, first) = (last.trim(), first.trim());
            let suffix = matches!(
                first.trim_end_matches('.').to_lowercase().as_str(),
                "jr" | "sr" | "ii" | "iii" | "iv"
            );
            if last.is_empty() || first.is_empty() || suffix {
                collapsed
            } else {
                format!("{first} {last}")
            }
        }
        _ => collapsed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(raw: &[&str]) -> Vec<String> {
        normalize_authors(
            &raw.iter()
                .map(std::string::ToString::to_string)
                .collect::<Vec<_>>(),
        )
    }

    #[test]
    fn splits_and_flips() {
        assert_eq!(
            n(&["Milton, John; Pullman, Philip;"]),
            ["John Milton", "Philip Pullman"]
        );
        assert_eq!(
            n(&["Terry Pratchett & Neil Gaiman"]),
            ["Terry Pratchett", "Neil Gaiman"]
        );
        assert_eq!(n(&["Taylor, Dennis E."]), ["Dennis E. Taylor"]);
    }

    #[test]
    fn leaves_ambiguous_names_alone() {
        assert_eq!(n(&["Martin Luther King, Jr."]), ["Martin Luther King, Jr."]);
        assert_eq!(n(&["A, B, C"]), ["A, B, C"]);
        assert_eq!(
            n(&["Brandon Sanderson", "brandon sanderson", "  "]),
            ["Brandon Sanderson"]
        );
    }
}
