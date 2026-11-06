import { describe, expect, it } from "vitest";
import type { Book, ImportResult } from "./api";
import {
  applyProgress,
  filterBooks,
  fromForm,
  defaultOrder,
  isbnOf,
  languageLabel,
  libraryColumns,
  pagesLabel,
  publishedLabel,
  summarizeImport,
  toForm,
} from "./library";
import { applySort, nextSort, parseSort, type Sort } from "./sort";

const book = (
  id: number,
  title: string,
  authors: string[],
  extra: Partial<Book> = {},
): Book => ({
  id,
  title,
  authors,
  series: null,
  series_index: null,
  tags: [],
  language: null,
  publisher: null,
  published: null,
  description: null,
  identifiers: [],
  words: null,
  pages: null,
  pages_estimated: true,
  original_name: `${title}.epub`,
  file_size: 1,
  has_cover: false,
  custom_cover: false,
  cover_upscaled: false,
  added_at: "2026-10-31T00:00:00Z",
  modified_at: "2026-10-31T00:00:00Z",
  cover: null,
  ...extra,
});

describe("filterBooks", () => {
  const books = [
    book(1, "Dune", ["Frank Herbert"], { series: "Dune Chronicles" }),
    book(2, "Emma", ["Jane Austen"], { tags: ["Classics"] }),
  ];
  it("matches every term across title, authors, series and tags", () => {
    expect(filterBooks(books, "herbert chron").map((b) => b.id)).toEqual([1]);
    expect(filterBooks(books, "CLASSICS").map((b) => b.id)).toEqual([2]);
    expect(filterBooks(books, "  ")).toHaveLength(2);
    expect(filterBooks(books, "dune austen")).toHaveLength(0);
  });
});

describe("applyProgress", () => {
  it("maps batch indexes to book ids", () => {
    let s = applyProgress({}, [7, 9], {
      stage: "converting",
      index: 2,
      total: 2,
      file: "x",
    });
    expect(s).toEqual({ 9: { stage: "converting" } });
    s = applyProgress(s, [7, 9], {
      stage: "failed",
      index: 2,
      total: 2,
      file: "x",
      error: "boom",
    });
    s = applyProgress(s, [7, 9], {
      stage: "done",
      index: 1,
      total: 2,
      file: "y",
      device_path: "KEpub/y",
    });
    expect(s).toEqual({
      7: { stage: "sent" },
      9: { stage: "failed", error: "boom" },
    });
  });
});

describe("summarizeImport", () => {
  it("summarises mixed outcomes", () => {
    const b = book(1, "A", []);
    const results: ImportResult[] = [
      {
        path: "/a.epub",
        outcome: {
          Ok: { kind: "added", book: b, possible_duplicate_of: null },
        },
      },
      {
        path: "/b.epub",
        outcome: { Ok: { kind: "added", book: b, possible_duplicate_of: 1 } },
      },
      {
        path: "/c.epub",
        outcome: { Ok: { kind: "already_in_library", book: b } },
      },
      { path: "/x/bad.epub", outcome: { Err: "not a zip" } },
    ];
    expect(summarizeImport(results)).toBe(
      "Added 2 books · 1 may duplicate books you already have · 1 already in your library · couldn't read bad.epub",
    );
  });
});

describe("edit form", () => {
  it("round-trips and cleans lists; drops index without series", () => {
    const b = book(1, "Dune", ["Frank Herbert"], {
      series: "Dune",
      series_index: "1",
      tags: ["SF"],
      contributors: [{ name: "Ann", role: "trl" }],
    });
    expect(fromForm(toForm(b))).toEqual({
      title: "Dune",
      authors: ["Frank Herbert"],
      series: "Dune",
      series_index: "1",
      tags: ["SF"],
      contributors: [{ name: "Ann", role: "trl" }],
    });
    expect(
      fromForm({
        title: " T ",
        authors: "A; B,, ",
        series: " ",
        seriesIndex: "3",
        genres: [],
        tags: "",
        contributors: [
          { name: " Bo ", role: "" },
          { name: " ", role: "ill" },
        ],
        contributorsKnown: true,
      }),
    ).toEqual({
      title: "T",
      authors: ["A", "B"],
      series: null,
      series_index: null,
      tags: [],
      contributors: [{ name: "Bo", role: "ctb" }],
    });
  });

  it("keeps genres in their own field and writes them back as tags", () => {
    const b = book(1, "Dune", ["Frank Herbert"], {
      tags: ["Favourites", "science fiction", "Classics", "Science Fiction"],
    });
    const f = toForm(b);
    expect(f.genres).toEqual(["Science Fiction", "Classics"]);
    expect(f.tags).toBe("Favourites");
    expect(fromForm(f).tags).toEqual([
      "Science Fiction",
      "Classics",
      "Favourites",
    ]);
  });

  it("leaves unknown contributors alone", () => {
    const f = toForm(book(1, "T", ["A"], { contributors: null }));
    expect(fromForm(f).contributors).toBeNull();
    // Clearing known contributors is sent as an empty list.
    const known = toForm(book(1, "T", ["A"], { contributors: [] }));
    expect(fromForm(known).contributors).toEqual([]);
  });
});

describe("sorting", () => {
  const books = [
    book(1, "The Hobbit", ["J. R. R. Tolkien"], {
      added_at: "2026-10-01T00:00:00Z",
    }),
    book(2, "Dune Messiah", ["Frank Herbert"], {
      series: "Dune",
      series_index: "2",
      added_at: "2026-10-03T00:00:00Z",
    }),
    book(3, "Dune", ["Frank Herbert"], {
      series: "Dune",
      series_index: "1",
      added_at: "2026-10-02T00:00:00Z",
    }),
    book(4, "Emma", ["Jane Austen"], { added_at: "2026-10-04T00:00:00Z" }),
  ];
  const columns = libraryColumns((b) =>
    b.id === 2 ? "stale" : b.id === 4 ? "on" : undefined,
  );
  type Key = Parameters<typeof nextSort>[1];
  const titles = (sort: Sort<Key>) =>
    applySort(defaultOrder(books), sort as never, columns).map((b) => b.title);

  it("defaults to title order ignoring leading articles", () => {
    expect(titles(null)).toEqual([
      "Dune",
      "Dune Messiah",
      "Emma",
      "The Hobbit",
    ]);
  });
  it("cycles descending, ascending, default", () => {
    let s = nextSort<Key>(null, "title");
    expect(s).toEqual({ key: "title", dir: "desc" });
    s = nextSort(s, "title");
    expect(s).toEqual({ key: "title", dir: "asc" });
    expect(nextSort(s, "title")).toBeNull();
    expect(nextSort(s, "author")).toEqual({ key: "author", dir: "desc" });
  });
  it("sorts authors by surname", () => {
    expect(titles({ key: "author", dir: "asc" })).toEqual([
      "Emma",
      "Dune",
      "Dune Messiah",
      "The Hobbit",
    ]);
  });
  it("keeps series in reading order and books without one last", () => {
    expect(titles({ key: "series", dir: "asc" })).toEqual([
      "Dune",
      "Dune Messiah",
      "Emma",
      "The Hobbit",
    ]);
    expect(titles({ key: "series", dir: "desc" })).toEqual([
      "Dune Messiah",
      "Dune",
      "Emma",
      "The Hobbit",
    ]);
  });
  it("puts recently added first when descending", () => {
    expect(titles({ key: "added", dir: "desc" })).toEqual([
      "Emma",
      "Dune Messiah",
      "Dune",
      "The Hobbit",
    ]);
  });
  it("puts books needing an update first, then books on the Kobo", () => {
    expect(titles({ key: "kobo", dir: "desc" })).toEqual([
      "Dune Messiah",
      "Emma",
      "Dune",
      "The Hobbit",
    ]);
  });
  it("ignores a stored sort for a column that no longer exists", () => {
    const keys = columns.map((c) => c.key);
    expect(parseSort('{"key":"added","dir":"asc"}', keys)).toEqual({
      key: "added",
      dir: "asc",
    });
    expect(parseSort('{"key":"rating","dir":"asc"}', keys)).toBeNull();
    expect(parseSort("title", keys)).toBeNull();
  });
});

describe("book details", () => {
  it("sorts longest first, with unmeasured books last either way", () => {
    const books = [
      book(1, "Short", [], { pages: 90 }),
      book(2, "Unknown", []),
      book(3, "Long", [], { pages: 900 }),
    ];
    const titles = (dir: "asc" | "desc") =>
      applySort(
        books,
        { key: "pages", dir },
        libraryColumns(() => undefined),
      ).map((b) => b.title);
    expect(titles("desc")).toEqual(["Long", "Short", "Unknown"]);
    expect(titles("asc")).toEqual(["Short", "Long", "Unknown"]);
  });

  it("says when a page count is an estimate", () => {
    const pages = (extra: Partial<Book>) => pagesLabel(book(1, "T", [], extra));
    expect(pages({ pages: 412, pages_estimated: false })).toBe("412");
    expect(pages({ pages: 412, pages_estimated: true })).toBe("About 412");
    expect(pages({ pages: null })).toBeNull();
  });

  it("shows publication dates without the unreliable day", () => {
    expect(publishedLabel("1965")).toBe("1965");
    // Late on the last day of the month stays in that month in any time zone.
    expect(publishedLabel("2020-06-30T22:00:00+00:00")).toMatch(/June 2020/);
    expect(publishedLabel("2020-01-01")).toMatch(/January 2020/);
  });

  it("names languages and keeps unknown codes", () => {
    expect(languageLabel("en")).toBe("English");
    expect(languageLabel("not a code")).toBe("not a code");
  });

  it("finds the ISBN however it's stored", () => {
    const isbn = (scheme: string | null, value: string) =>
      isbnOf(book(1, "T", [], { identifiers: [{ scheme, value }] }));
    expect(isbn("ISBN", "978")).toBe("978");
    expect(isbn(null, "isbn:978")).toBe("978");
    expect(isbn("uuid", "978")).toBeNull();
  });
});
