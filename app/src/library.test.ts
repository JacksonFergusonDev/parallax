import { describe, expect, it } from "vitest";
import type { Book, ImportResult } from "./api";
import {
  applyProgress,
  filterBooks,
  fromForm,
  summarizeImport,
  toForm,
} from "./library";

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
  description: null,
  identifiers: [],
  original_name: `${title}.epub`,
  file_size: 1,
  has_cover: false,
  custom_cover: false,
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
    });
    expect(fromForm(toForm(b))).toEqual({
      title: "Dune",
      authors: ["Frank Herbert"],
      series: "Dune",
      series_index: "1",
      tags: ["SF"],
    });
    expect(
      fromForm({
        title: " T ",
        authors: "A; B,, ",
        series: " ",
        seriesIndex: "3",
        tags: "",
      }),
    ).toEqual({
      title: "T",
      authors: ["A", "B"],
      series: null,
      series_index: null,
      tags: [],
    });
  });
});
