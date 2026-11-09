//! Series, volume and chapter from a comic's file name, for files without
//! ComicInfo.xml (most of them): `Berserk v01 (2003) (Digital)
//! (danke-Empire).cbz`, `[Group] One Piece - Vol. 12.cbz`, `Chainsaw Man
//! c097.cbz`, `Saga #12.cbr`, `Batman 001 (2016).cbz`.

/// What a file name says. Numbers are as written, without leading zeros.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NameInfo {
    pub series: String,
    pub volume: Option<String>,
    pub chapter: Option<String>,
    /// An issue number (`#12`) or a bare trailing number (`Batman 001`).
    pub number: Option<String>,
    pub year: Option<u16>,
    /// Text after ` - ` following the numbers (`v01 - The Black Swordsman`).
    pub title: Option<String>,
}

const EXTENSIONS: [&str; 6] = [".cbz", ".cbr", ".cb7", ".zip", ".rar", ".7z"];

pub fn parse_file_name(name: &str) -> NameInfo {
    let lower = name.to_lowercase();
    let stem = EXTENSIONS
        .iter()
        .find(|ext| lower.ends_with(*ext))
        .map_or(name, |ext| &name[..name.len() - ext.len()]);

    let (text, groups) = strip_groups(stem);
    let year = groups.iter().find_map(|g| year_in(g));
    let words: Vec<String> = text
        .replace('_', " ")
        .split_whitespace()
        .map(str::to_owned)
        .collect();

    let mut info = NameInfo {
        year,
        ..NameInfo::default()
    };
    let mut first_marker = None;
    let mut last_marker = None;
    let mut i = 0;
    while i < words.len() {
        let consumed = marker(&words, i, &mut info);
        if consumed > 0 {
            first_marker.get_or_insert(i);
            last_marker = Some(i + consumed);
            i += consumed;
        } else {
            i += 1;
        }
    }

    let series_end = first_marker.unwrap_or(words.len());
    info.series = join_trimmed(&words[..series_end]);
    if let Some(end) = last_marker {
        let rest = &words[end..];
        if rest.first().is_some_and(|w| is_dash(w)) {
            let title = join_trimmed(&rest[1..]);
            if !title.is_empty() {
                info.title = Some(title);
            }
        }
    }
    if info.series.is_empty() {
        info.series = join_trimmed(&words);
    }
    info
}

/// Recognises a volume, chapter or number at `words[i]`, records it, and
/// returns how many words it used (0 when it isn't one).
fn marker(words: &[String], i: usize, info: &mut NameInfo) -> usize {
    let word = words[i].to_lowercase();
    let next = words.get(i + 1).and_then(|w| number(w));
    let slot = |prefixes: &[&str], spelled: &[&str]| -> Option<(String, usize)> {
        for p in prefixes {
            if let Some(n) = word.strip_prefix(p).and_then(number) {
                return Some((n, 1));
            }
        }
        if spelled.contains(&word.as_str()) {
            return next.clone().map(|n| (n, 2));
        }
        None
    };
    if info.volume.is_none()
        && let Some((n, used)) = slot(&["vol.", "vol", "v"], &["vol", "vol.", "volume", "v."])
    {
        info.volume = Some(n);
        return used;
    }
    if info.chapter.is_none()
        && let Some((n, used)) = slot(
            &["ch.", "ch", "c"],
            &["ch", "ch.", "chapter", "chap", "chap."],
        )
    {
        info.chapter = Some(n);
        return used;
    }
    if info.number.is_none() {
        if let Some(n) = word.strip_prefix('#').and_then(number) {
            info.number = Some(n);
            return 1;
        }
        // A bare number after the series, but not one that starts the name.
        if i > 0
            && let Some(n) = number(&word)
            && (info.volume.is_some() || info.chapter.is_some() || is_last_or_before_dash(words, i))
        {
            info.number = Some(n);
            return 1;
        }
    }
    0
}

fn is_last_or_before_dash(words: &[String], i: usize) -> bool {
    words.get(i + 1).is_none_or(|w| is_dash(w))
}

fn is_dash(word: &str) -> bool {
    matches!(word, "-" | "–" | "—")
}

/// `007` → `7`, `012.5` → `12.5`; anything else isn't a number.
fn number(s: &str) -> Option<String> {
    let (whole, frac) = s.split_once('.').map_or((s, None), |(w, f)| (w, Some(f)));
    if whole.is_empty() || !whole.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    if frac.is_some_and(|f| f.is_empty() || !f.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    let whole = whole.trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    Some(match frac {
        Some(f) => format!("{whole}.{f}"),
        None => whole.to_owned(),
    })
}

/// Removes `(…)`, `[…]` and `{…}` groups, returning the rest and the groups.
fn strip_groups(s: &str) -> (String, Vec<String>) {
    let mut text = String::new();
    let mut groups = Vec::new();
    let mut current = String::new();
    let mut close: Option<char> = None;
    for c in s.chars() {
        match close {
            Some(end) if c == end => {
                groups.push(std::mem::take(&mut current));
                close = None;
                text.push(' ');
            }
            Some(_) => current.push(c),
            None => match c {
                '(' => close = Some(')'),
                '[' => close = Some(']'),
                '{' => close = Some('}'),
                _ => text.push(c),
            },
        }
    }
    if close.is_some() {
        // Unbalanced: keep what followed the bracket as text.
        text.push_str(&current);
    }
    (text, groups)
}

fn year_in(group: &str) -> Option<u16> {
    let g = group.trim();
    let year: u16 = g.get(..4)?.parse().ok()?;
    let rest = &g[4..];
    ((1900..=2099).contains(&year) && (rest.is_empty() || rest.starts_with(['-', ' ', '.'])))
        .then_some(year)
}

fn join_trimmed(words: &[String]) -> String {
    let mut words = words;
    while words.last().is_some_and(|w| is_dash(w)) {
        words = &words[..words.len() - 1];
    }
    while words.first().is_some_and(|w| is_dash(w)) {
        words = &words[1..];
    }
    words
        .join(" ")
        .trim_end_matches([',', '-', ':'])
        .trim()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(series: &str) -> NameInfo {
        NameInfo {
            series: series.to_owned(),
            ..NameInfo::default()
        }
    }

    #[test]
    fn scan_group_volumes() {
        assert_eq!(
            parse_file_name("Berserk v01 (2003) (Digital) (danke-Empire).cbz"),
            NameInfo {
                volume: Some("1".into()),
                year: Some(2003),
                ..info("Berserk")
            }
        );
    }

    #[test]
    fn spelled_out_volumes_and_titles() {
        assert_eq!(
            parse_file_name("[Group] One Piece - Vol. 12 - Showdown.cbz"),
            NameInfo {
                volume: Some("12".into()),
                title: Some("Showdown".into()),
                ..info("One Piece")
            }
        );
        assert_eq!(
            parse_file_name("Vagabond Volume 3.cbr"),
            NameInfo {
                volume: Some("3".into()),
                ..info("Vagabond")
            }
        );
    }

    #[test]
    fn chapters_issues_and_bare_numbers() {
        assert_eq!(
            parse_file_name("Chainsaw Man c097.cbz"),
            NameInfo {
                chapter: Some("97".into()),
                ..info("Chainsaw Man")
            }
        );
        assert_eq!(
            parse_file_name("Kingdom v05 c045.5.cbz"),
            NameInfo {
                volume: Some("5".into()),
                chapter: Some("45.5".into()),
                ..info("Kingdom")
            }
        );
        assert_eq!(
            parse_file_name("Saga #12.cbr"),
            NameInfo {
                number: Some("12".into()),
                ..info("Saga")
            }
        );
        assert_eq!(
            parse_file_name("Batman 001 (2016) (digital).cbz"),
            NameInfo {
                number: Some("1".into()),
                year: Some(2016),
                ..info("Batman")
            }
        );
    }

    #[test]
    fn numbers_inside_titles_stay_put() {
        assert_eq!(
            parse_file_name("20th Century Boys v01.cbz").series,
            "20th Century Boys"
        );
        assert_eq!(parse_file_name("2001 Nights.cbz"), info("2001 Nights"));
        assert_eq!(
            parse_file_name("Mob Psycho 100 v02.cbz").series,
            "Mob Psycho 100"
        );
    }

    #[test]
    fn underscores_and_odd_brackets() {
        assert_eq!(
            parse_file_name("Blame_v03_(2001).cbz"),
            NameInfo {
                volume: Some("3".into()),
                year: Some(2001),
                ..info("Blame")
            }
        );
        assert_eq!(parse_file_name("Odd (unclosed.cbz").series, "Odd unclosed");
        assert_eq!(parse_file_name("-dash first v1.cbz").series, "-dash first");
    }

    #[test]
    fn names_with_nothing_to_parse() {
        assert_eq!(parse_file_name("Akira.cbz"), info("Akira"));
        assert_eq!(parse_file_name("v01.cbz").volume.as_deref(), Some("1"));
    }
}
