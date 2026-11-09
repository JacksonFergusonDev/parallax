// The standard genre names, in the backend's order (parallax_metadata::genres;
// a Rust test checks the two lists match). A tag with one of these names is a
// genre; the edit panel shows those in their own field.

export const GENRES: readonly string[] = [
  "Historical Fiction",
  "Science Fiction",
  "Fantasy",
  "Horror",
  "Mystery & Thriller",
  "Romance",
  "Humor",
  "Young Adult",
  "Classics",
  "Literary Fiction",
  "Biography & Memoir",
  "Travel & Exploration",
  "History",
  "Politics & Society",
  "Science & Nature",
  "Self-Help & Psychology",
  "Philosophy",
  "Business & Economics",
  "Poetry",
  "Nonfiction",
  "Fiction",
];

const byKey = new Map(GENRES.map((g) => [g.toLowerCase(), g]));

/** The standard genre this tag is, if its name is one (ignoring case). */
export const genreOf = (tag: string): string | undefined =>
  byKey.get(tag.trim().toLowerCase());

/** Splits tags into genres (standard names) and the rest. */
export function splitGenres(tags: string[]): {
  genres: string[];
  others: string[];
} {
  const genres: string[] = [];
  const others: string[] = [];
  for (const t of tags) {
    const g = genreOf(t);
    if (g) {
      if (!genres.includes(g)) genres.push(g);
    } else others.push(t);
  }
  return { genres, others };
}
