//! A short, fixed genre list, and mapping the free-form subjects and
//! categories sources return (Open Library subjects, Apple and Google
//! categories, Wikidata genres) onto it.

/// Fiction or not; a book gets a broad genre only when nothing more
/// specific was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Fiction,
    Nonfiction,
    Other,
}

pub struct Genre {
    pub name: &'static str,
    pub kind: Kind,
    /// Broad catch-alls ("Fiction", "Nonfiction").
    pub broad: bool,
    /// Lowercase word beginnings ("biograph" matches "biographies", "teen"
    /// doesn't match "eighteenth"); the first genre with a match wins, so
    /// more specific genres come first.
    keywords: &'static [&'static str],
}

const fn g(
    name: &'static str,
    kind: Kind,
    broad: bool,
    keywords: &'static [&'static str],
) -> Genre {
    Genre {
        name,
        kind,
        broad,
        keywords,
    }
}

pub const GENRES: &[Genre] = &[
    g(
        "Historical Fiction",
        Kind::Fiction,
        false,
        &["historical fiction", "fiction, historical"],
    ),
    g(
        "Science Fiction",
        Kind::Fiction,
        false,
        &[
            "science fiction",
            "sci-fi",
            "scifi",
            "space opera",
            "cyberpunk",
            "dystopia",
        ],
    ),
    g("Fantasy", Kind::Fiction, false, &["fantasy"]),
    g("Horror", Kind::Fiction, false, &["horror", "ghost stories"]),
    g(
        "Mystery & Thriller",
        Kind::Fiction,
        false,
        &[
            "mystery",
            "thriller",
            "detective",
            "crime fiction",
            "suspense",
        ],
    ),
    g(
        "Romance",
        Kind::Fiction,
        false,
        &["romance", "love stories"],
    ),
    g(
        "Humor",
        Kind::Fiction,
        false,
        &["humor", "humour", "comedy", "humorous", "parody", "satire"],
    ),
    g(
        "Young Adult",
        Kind::Fiction,
        false,
        &["young adult", "juvenile fiction", "teen"],
    ),
    g(
        "Classics",
        Kind::Fiction,
        false,
        &["classics", "classic literature"],
    ),
    g(
        "Literary Fiction",
        Kind::Fiction,
        false,
        &["literary fiction"],
    ),
    g(
        "Biography & Memoir",
        Kind::Nonfiction,
        false,
        &["biograph", "memoir"],
    ),
    g(
        "Travel & Exploration",
        Kind::Nonfiction,
        false,
        &[
            "travel",
            "discovery and exploration",
            "explorers",
            "expeditions",
        ],
    ),
    g("History", Kind::Nonfiction, false, &["history"]),
    // Before Science, so "Political Science" and "Social Science" land here.
    g(
        "Politics & Society",
        Kind::Nonfiction,
        false,
        &[
            "political",
            "politics",
            "social science",
            "sociology",
            "communism",
            "capitalism",
            "social justice",
            "race relations",
        ],
    ),
    g(
        "Science & Nature",
        Kind::Nonfiction,
        false,
        &[
            "science",
            "physics",
            "biology",
            "astronomy",
            "mathematics",
            "nature",
        ],
    ),
    g(
        "Self-Help & Psychology",
        Kind::Nonfiction,
        false,
        &["self-help", "psychology", "personal development"],
    ),
    g("Philosophy", Kind::Nonfiction, false, &["philosophy"]),
    g(
        "Business & Economics",
        Kind::Nonfiction,
        false,
        &["business", "economics", "finance"],
    ),
    g("Poetry", Kind::Other, false, &["poetry"]),
    g(
        "Nonfiction",
        Kind::Nonfiction,
        true,
        &["nonfiction", "non-fiction"],
    ),
    g("Fiction", Kind::Fiction, true, &["fiction", "novel"]),
];

/// The genre a subject or category belongs to, if any.
pub fn classify(subject: &str) -> Option<&'static Genre> {
    let s = subject.to_lowercase();
    // Umbrella categories that name two genres say nothing about either.
    if s.contains("sci") && s.contains("fantasy") && !s.contains("science fiction") {
        return None;
    }
    GENRES
        .iter()
        .find(|genre| genre.keywords.iter().any(|k| starts_word(&s, k)))
}

fn starts_word(text: &str, fragment: &str) -> bool {
    text.match_indices(fragment).any(|(i, _)| {
        text[..i]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric())
    })
}

pub fn by_name(name: &str) -> Option<&'static Genre> {
    GENRES.iter().find(|g| g.name.eq_ignore_ascii_case(name))
}

/// A folder or tag name that means a genre, e.g. "Sci-Fi" → Science Fiction.
pub fn from_label(label: &str) -> Option<&'static Genre> {
    by_name(label).or_else(|| classify(label))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(s: &str) -> Option<&'static str> {
        classify(s).map(|g| g.name)
    }

    #[test]
    fn maps_subjects_from_each_source() {
        // Open Library
        assert_eq!(name("Fiction, fantasy, epic"), Some("Fantasy"));
        assert_eq!(
            name("Discovery and exploration"),
            Some("Travel & Exploration")
        );
        assert_eq!(
            name("Fiction, historical, general"),
            Some("Historical Fiction")
        );
        // Apple
        assert_eq!(name("Epic Fantasy"), Some("Fantasy"));
        assert_eq!(name("Biographies & Memoirs"), Some("Biography & Memoir"));
        assert_eq!(name("Fiction & Literature"), Some("Fiction"));
        assert_eq!(name("Sci-Fi & Fantasy"), None);
        assert_eq!(name("Books"), None);
        assert_eq!(name("Great Britain, eighteenth century"), None);
        assert_eq!(name("Non-fiction"), Some("Nonfiction"));
        // Wikidata
        assert_eq!(name("Political Science"), Some("Politics & Society"));
        assert_eq!(name("Social Science"), Some("Politics & Society"));
        assert_eq!(name("Popular works of science"), Some("Science & Nature"));
        assert_eq!(name("speculative fiction comedy"), Some("Humor"));
        assert_eq!(name("science fiction novel"), Some("Science Fiction"));
    }

    #[test]
    fn folder_names_map_to_genres() {
        assert_eq!(
            from_label("Sci-Fi").map(|g| g.name),
            Some("Science Fiction")
        );
        assert_eq!(from_label("Nonfiction").map(|g| g.name), Some("Nonfiction"));
        assert_eq!(from_label("Fantasy").map(|g| g.name), Some("Fantasy"));
        assert!(from_label("To read").is_none());
    }
}
