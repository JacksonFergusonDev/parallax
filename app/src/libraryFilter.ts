// Narrowing the library by Kobo status, collection, genre and reading status.
import type { Book } from "./api";
import { splitGenres } from "./genres";
import type { DeviceState } from "./library";
import { collator } from "./sort";

export type KoboFilter = "all" | "on" | "off" | "stale";
export type ReadingFilter = "unread" | "reading" | "finished";

export interface LibraryFilter {
  kobo: KoboFilter;
  collection: string | null;
  genre: string | null;
  reading: ReadingFilter | null;
}

export const NO_FILTER: LibraryFilter = {
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

export function filterLibrary(
  books: Book[],
  f: LibraryFilter,
  facts: BookFacts,
): Book[] {
  if (!isFiltered(f)) return books;
  return books.filter(
    (b) =>
      matchesKobo(facts.deviceState(b), f.kobo) &&
      (f.collection === null ||
        facts.collectionsOf(b).includes(f.collection)) &&
      (f.genre === null || splitGenres(b.tags).genres.includes(f.genre)) &&
      (f.reading === null || readingOf(b) === f.reading),
  );
}

export type Counted = readonly [name: string, count: number];

export interface Facets {
  kobo: Record<KoboFilter, number>;
  collections: Counted[];
  genres: Counted[];
  reading: Record<ReadingFilter, number>;
}

/** Counts by name, by collation; `current` stays listed even with no books. */
function tally(
  books: Book[],
  namesOf: (b: Book) => readonly string[],
  current: string | null,
): Counted[] {
  const counts = new Map<string, number>();
  if (current !== null) counts.set(current, 0);
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
    collections: tally(
      others({ collection: null }),
      facts.collectionsOf,
      f.collection,
    ),
    genres: tally(
      others({ genre: null }),
      (b) => splitGenres(b.tags).genres,
      f.genre,
    ),
    reading: {
      unread: readingCount("unread"),
      reading: readingCount("reading"),
      finished: readingCount("finished"),
    },
  };
}
