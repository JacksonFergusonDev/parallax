// Saved searches: a search and filters kept under a name, like calibre's
// saved searches. Kept as JSON in the library's `saved_searches` setting so
// they survive reinstalls with the library.
import { NO_FILTER, type LibraryFilter } from "./libraryFilter";

export interface SavedSearch {
  name: string;
  query: string;
  filter: LibraryFilter;
}

/** The setting that turns them on, and the one that holds them. */
export const SAVED_SEARCHES_ON = "library_saved_searches";
export const SAVED_SEARCHES = "saved_searches";

/** Saved searches from the setting; anything unreadable is left out. */
export function parseSaved(value: string | null | undefined): SavedSearch[] {
  if (!value) return [];
  try {
    const list: unknown = JSON.parse(value);
    if (!Array.isArray(list)) return [];
    return list.flatMap((item) => {
      if (typeof item !== "object" || item === null) return [];
      const { name, query, filter } = item as Record<string, unknown>;
      if (typeof name !== "string" || !name.trim()) return [];
      return [
        {
          name,
          query: typeof query === "string" ? query : "",
          filter: {
            ...NO_FILTER,
            ...(typeof filter === "object" && filter !== null ? filter : {}),
          } as LibraryFilter,
        },
      ];
    });
  } catch {
    return [];
  }
}

/** `list` with `search` added, replacing one of the same name. */
export function withSaved(
  list: SavedSearch[],
  search: SavedSearch,
): SavedSearch[] {
  const key = search.name.trim().toLocaleLowerCase();
  return [
    ...list.filter((s) => s.name.trim().toLocaleLowerCase() !== key),
    { ...search, name: search.name.trim() },
  ].sort((a, b) => a.name.localeCompare(b.name));
}

export const serializeSaved = (list: SavedSearch[]) =>
  list.length ? JSON.stringify(list) : null;

const sameFilter = (a: LibraryFilter, b: LibraryFilter) =>
  a.kind === b.kind &&
  a.kobo === b.kobo &&
  a.collection === b.collection &&
  a.genre === b.genre &&
  a.reading === b.reading;

/** The saved search the library is showing now, if any. */
export const activeSaved = (
  list: SavedSearch[],
  query: string,
  filter: LibraryFilter,
) =>
  list.find(
    (s) => s.query.trim() === query.trim() && sameFilter(s.filter, filter),
  ) ?? null;
