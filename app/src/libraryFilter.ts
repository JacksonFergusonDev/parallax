// Narrowing the library by Kobo status, collection, genre and reading status.
import type { Book } from "./api";
import { splitGenres } from "./genres";
import type { DeviceState } from "./library";
import { collator } from "./sort";

export type KoboFilter = "all" | "on" | "off" | "stale";
export type ReadingFilter = "unread" | "reading" | "finished";

/** As a collection or genre filter: books with none. No real name is this. */
export const NONE = "\0none";

export interface LibraryFilter {
  kind: Book["kind"] | null;
  kobo: KoboFilter;
  collection: string | null;
  genre: string | null;
  reading: ReadingFilter | null;
}

export const NO_FILTER: LibraryFilter = {
  kind: null,
  kobo: "all",
  collection: null,
  genre: null,
  reading: null,
};

/** What the filters need to know about a book beyond the book itself. */
export interface BookFacts {
  deviceState: (b: Book) => DeviceState;
  collectionsOf: (b: Book) => readonly string[];
}

export const isFiltered = (f: LibraryFilter) =>
  f.kind !== null ||
  f.kobo !== "all" ||
  f.collection !== null ||
  f.genre !== null ||
  f.reading !== null;

/** Kobo filters mean nothing with no Kobo connected, so they rest until one
 * returns. Genre and reading status come from the library and still apply. */
export const withoutKobo = (f: LibraryFilter): LibraryFilter => ({
  ...f,
  kobo: "all",
  collection: null,
});

/** A book no Kobo has reported on counts as unread. */
const readingOf = (b: Book): ReadingFilter =>
  b.reading?.status === "finished" || b.reading?.status === "reading"
    ? b.reading.status
    : "unread";

function matchesKobo(state: DeviceState, kobo: KoboFilter): boolean {
  switch (kobo) {
    case "all":
      return true;
    case "on":
      return state !== undefined;
    case "off":
      return state === undefined;
    case "stale":
      return state === "stale";
  }
}

function matchesName(names: readonly string[], choice: string | null): boolean {
  if (choice === null) return true;
  return choice === NONE ? names.length === 0 : names.includes(choice);
}

export function filterLibrary(
  books: Book[],
  f: LibraryFilter,
  facts: BookFacts,
): Book[] {
  if (!isFiltered(f)) return books;
  return books.filter(
    (b) =>
      (f.kind === null || (b.kind ?? "book") === f.kind) &&
      matchesKobo(facts.deviceState(b), f.kobo) &&
      matchesName(facts.collectionsOf(b), f.collection) &&
      matchesName(splitGenres(b.tags).genres, f.genre) &&
      (f.reading === null || readingOf(b) === f.reading),
  );
}

export type Counted = readonly [name: string, count: number];

export interface Facets {
  kobo: Record<KoboFilter, number>;
  collections: Counted[];
  /** Books in no collection. */
  noCollection: number;
  genres: Counted[];
  /** Books with no genre. */
  noGenre: number;
  reading: Record<ReadingFilter, number>;
}

/** Counts by name, by collation; `current` stays listed even with no books. */
function tally(
  books: Book[],
  namesOf: (b: Book) => readonly string[],
  current: string | null,
): Counted[] {
  const counts = new Map<string, number>();
  if (current !== null && current !== NONE) counts.set(current, 0);
  for (const b of books) {
    for (const name of namesOf(b)) {
      counts.set(name, (counts.get(name) ?? 0) + 1);
    }
  }
  return [...counts].sort(([a], [b]) => collator.compare(a, b));
}

/** What each control would show if picked: every count respects the other
 * filters, but not the control's own. */
export function libraryFacets(
  books: Book[],
  f: LibraryFilter,
  facts: BookFacts,
): Facets {
  const others = (without: Partial<LibraryFilter>) =>
    filterLibrary(books, { ...f, ...without }, facts);
  const forCollection = others({ collection: null });
  const forGenre = others({ genre: null });
  const genresOf = (b: Book) => splitGenres(b.tags).genres;
  const forKobo = others({ kobo: "all" });
  const states = forKobo.map(facts.deviceState);
  const forReading = others({ reading: null }).map(readingOf);
  const readingCount = (r: ReadingFilter) =>
    forReading.filter((x) => x === r).length;
  return {
    kobo: {
      all: states.length,
      on: states.filter((s) => s !== undefined).length,
      off: states.filter((s) => s === undefined).length,
      stale: states.filter((s) => s === "stale").length,
    },
    collections: tally(forCollection, facts.collectionsOf, f.collection),
    noCollection: forCollection.filter(
      (b) => facts.collectionsOf(b).length === 0,
    ).length,
    genres: tally(forGenre, genresOf, f.genre),
    noGenre: forGenre.filter((b) => genresOf(b).length === 0).length,
    reading: {
      unread: readingCount("unread"),
      reading: readingCount("reading"),
      finished: readingCount("finished"),
    },
  };
}
