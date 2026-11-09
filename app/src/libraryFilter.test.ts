import { describe, expect, it } from "vitest";
import type { Book } from "./api";
import type { DeviceCopyState } from "./library";
import {
  filterLibrary,
  isFiltered,
  libraryFacets,
  NO_FILTER,
  NONE,
  withoutKobo,
  type BookFacts,
} from "./libraryFilter";

const book = (id: number, tags: string[], reading?: string) =>
  ({
    id,
    tags,
    reading: reading ? { status: reading, percent: 50 } : null,
  }) as unknown as Book;

const dune = book(1, ["Science Fiction"], "reading");
const hobbit = book(2, ["fantasy", "dragons"], "finished");
const emma = book(3, ["Classics"], "unread");
const blank = book(4, []);
const books = [dune, hobbit, emma, blank];

const states: Record<number, DeviceCopyState> = { 1: "on", 2: "stale" };
const shelves: Record<number, string[]> = {
  1: ["Sci-Fi", "Best"],
  2: ["Best"],
};
const facts: BookFacts = {
  deviceState: (b) => states[b.id],
  collectionsOf: (b) => shelves[b.id] ?? [],
};

const ids = (filter: Partial<typeof NO_FILTER>) =>
  filterLibrary(books, { ...NO_FILTER, ...filter }, facts).map((b) => b.id);

describe("filterLibrary", () => {
  it("leaves the library alone with no filters", () => {
    expect(filterLibrary(books, NO_FILTER, facts)).toBe(books);
  });

  it("splits books by whether they're on the Kobo", () => {
    expect(ids({ kobo: "on" })).toEqual([1, 2]);
    expect(ids({ kobo: "off" })).toEqual([3, 4]);
  });

  it("finds books needing an update", () => {
    expect(ids({ kobo: "stale" })).toEqual([2]);
  });

  it("filters by collection", () => {
    expect(ids({ collection: "Best" })).toEqual([1, 2]);
    expect(ids({ collection: "Sci-Fi" })).toEqual([1]);
  });

  it("filters by genre, whatever the tag's case", () => {
    expect(ids({ genre: "Fantasy" })).toEqual([2]);
  });

  it("counts books no Kobo has seen as unread", () => {
    expect(ids({ reading: "unread" })).toEqual([3, 4]);
    expect(ids({ reading: "finished" })).toEqual([2]);
  });

  it("finds books in no collection, on the Kobo or not", () => {
    expect(ids({ collection: NONE })).toEqual([3, 4]);
    states[3] = "on";
    expect(ids({ collection: NONE, kobo: "on" })).toEqual([3]);
    delete states[3];
  });

  it("finds books with no genre, even with other tags", () => {
    const tagged = book(5, ["dragons"]);
    const found = filterLibrary(
      [...books, tagged],
      { ...NO_FILTER, genre: NONE },
      facts,
    );
    expect(found.map((b) => b.id)).toEqual([4, 5]);
  });

  it("combines filters", () => {
    expect(ids({ collection: "Best", reading: "reading" })).toEqual([1]);
    expect(ids({ kobo: "off", genre: "Fantasy" })).toEqual([]);
  });
});

describe("withoutKobo", () => {
  it("keeps the filters that don't need a Kobo", () => {
    const filter = {
      ...NO_FILTER,
      kind: "manga",
      kobo: "on",
      collection: "Best",
      genre: "Fantasy",
      reading: "finished",
    } as const;
    expect(withoutKobo(filter)).toEqual({
      ...NO_FILTER,
      kind: "manga",
      genre: "Fantasy",
      reading: "finished",
    });
  });
});

describe("isFiltered", () => {
  it("is false only with nothing chosen", () => {
    expect(isFiltered(NO_FILTER)).toBe(false);
    expect(isFiltered({ ...NO_FILTER, kobo: "off" })).toBe(true);
    expect(isFiltered({ ...NO_FILTER, genre: "Poetry" })).toBe(true);
  });
});

describe("libraryFacets", () => {
  it("counts every option", () => {
    const facets = libraryFacets(books, NO_FILTER, facts);
    expect(facets.kobo).toEqual({ all: 4, on: 2, off: 2, stale: 1 });
    expect(facets.collections).toEqual([
      ["Best", 2],
      ["Sci-Fi", 1],
    ]);
    expect(facets.genres).toEqual([
      ["Classics", 1],
      ["Fantasy", 1],
      ["Science Fiction", 1],
    ]);
    expect(facets.reading).toEqual({ unread: 2, reading: 1, finished: 1 });
  });

  it("counts a control's options without its own filter", () => {
    const facets = libraryFacets(books, { ...NO_FILTER, kobo: "off" }, facts);
    expect(facets.kobo).toEqual({ all: 4, on: 2, off: 2, stale: 1 });
  });

  it("counts other controls within the current filters", () => {
    const facets = libraryFacets(books, { ...NO_FILTER, kobo: "off" }, facts);
    expect(facets.genres).toEqual([["Classics", 1]]);
    expect(facets.collections).toEqual([]);
  });

  it("counts books with no collection or genre", () => {
    const facets = libraryFacets(books, NO_FILTER, facts);
    expect(facets.noCollection).toBe(2);
    expect(facets.noGenre).toBe(1);
  });

  it("doesn't list the none choice as a name", () => {
    const filter = { ...NO_FILTER, genre: NONE, collection: NONE };
    const facets = libraryFacets(books, filter, facts);
    expect(facets.genres.map(([name]) => name)).not.toContain(NONE);
    expect(facets.collections.map(([name]) => name)).not.toContain(NONE);
  });

  it("keeps the chosen option listed when nothing matches it", () => {
    const filter = { ...NO_FILTER, kobo: "off", genre: "Fantasy" } as const;
    expect(libraryFacets(books, filter, facts).genres).toContainEqual([
      "Fantasy",
      0,
    ]);
  });
});
