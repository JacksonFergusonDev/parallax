import { describe, expect, it } from "vitest";
import type { GoodreadsInsights } from "./api";
import {
  decadeBuckets,
  importLine,
  insightsHeadline,
  lengthBars,
  spokenDays,
  tableRows,
  type InsightBook,
} from "./goodreadsInsights";
import { emptyInsights } from "./dev/goodreadsMock";

const book = (over: Partial<InsightBook>): InsightBook => ({
  goodreads_id: "1",
  title: "A",
  series: null,
  authors: ["Ann"],
  isbn: null,
  isbn13: null,
  publisher: null,
  binding: null,
  pages: null,
  year_published: null,
  original_year: null,
  rating: null,
  shelf: "read",
  shelves: [],
  date_read: null,
  date_added: null,
  read_count: 1,
  review: null,
  library: [],
  ...over,
});

const insights = (over: Partial<GoodreadsInsights>): GoodreadsInsights => ({
  ...emptyInsights(),
  ...over,
});

describe("insightsHeadline", () => {
  it("compares this year with the same point last year", () => {
    const pace = {
      year: 2026,
      this_year: 5,
      last_year_by_now: 3,
      last_year: 9,
    };
    expect(insightsHeadline(insights({ pace }))).toBe(
      "5 books read so far this year, against 3 by now last year.",
    );
    expect(
      insightsHeadline(insights({ pace: { ...pace, last_year_by_now: 5 } })),
    ).toBe("5 books read so far this year.");
  });

  it("falls back to the totals with nothing read this year", () => {
    const totals = { ...emptyInsights().totals, read: 1, to_read: 166 };
    expect(insightsHeadline(insights({ totals }))).toBe(
      "1 book read, and 166 more waiting.",
    );
  });
});

describe("importLine", () => {
  it("dates the export by its file and counts imports", () => {
    const line = importLine(
      {
        imported_at: "2026-11-12T10:00:00Z",
        file_name: "goodreads_library_export.csv",
        file_modified: "2026-11-03T10:00:00Z",
        latest_activity: "2026-11-02",
        books: 283,
        added: 283,
        updated: 0,
      },
      2,
    );
    expect(line).toMatch(
      /^From your Goodreads export of .*2026 · imported .*2026 · 2 imports so far$/,
    );
  });
});

describe("importLine without the file's date", () => {
  it("dates the export by its latest activity", () => {
    const line = importLine(
      {
        imported_at: "2026-11-12T10:00:00Z",
        file_name: null,
        file_modified: null,
        latest_activity: "2026-11-02",
        books: 1,
        added: 1,
        updated: 0,
      },
      1,
    );
    expect(line).toMatch(
      /^From your Goodreads export, with activity up to .*2026 · imported .*2026$/,
    );
  });
});

describe("spokenDays", () => {
  it.each([
    [1, "1 day"],
    [20, "3 weeks"],
    [150, "5 months"],
    [800, "2 years"],
    [1000, "2.5 years"],
  ])("%d days is %s", (days, text) => expect(spokenDays(days)).toBe(text));
});

describe("chart groups", () => {
  it("names decades in full and everything before 1900 together", () => {
    const labels = decadeBuckets([
      { decade: null, read: 1, to_read: 0 },
      { decade: 1900, read: 1, to_read: 1 },
      { decade: 2000, read: 0, to_read: 2 },
    ]).map((b) => [b.label, b.count]);
    expect(labels).toEqual([
      ["Earlier", 1],
      ["1900s", 2],
      ["2000s", 2],
    ]);
  });

  it("labels length ranges", () => {
    const bars = lengthBars(
      [
        { min: 0, max: 200, read: 1, to_read: 0 },
        { min: 200, max: 300, read: 2, to_read: 0 },
        { min: 700, max: null, read: 3, to_read: 0 },
      ],
      "read",
    ).map((b) => b.label);
    expect(bars).toEqual(["Under 200", "200–299", "700+ pages"]);
  });
});

describe("tableRows", () => {
  const rows = [
    book({
      goodreads_id: "1",
      title: "Beta",
      rating: 4,
      date_read: "2024-01-01",
    }),
    book({
      goodreads_id: "2",
      title: "Alpha",
      rating: null,
      shelf: "to_read",
      date_added: "2025-01-01",
    }),
    book({
      goodreads_id: "3",
      title: "Gamma",
      rating: 5,
      authors: ["Zed"],
      series: { name: "Saga", number: "1" },
    }),
  ];
  const ids = (r: InsightBook[]) => r.map((b) => b.goodreads_id);
  const all = { status: "all", inLibrary: false } as const;

  it("keeps to books in your library when asked", () => {
    const owned = [
      ...rows,
      book({ goodreads_id: "4", title: "Delta", library: [7] }),
    ];
    expect(
      ids(
        tableRows(owned, { status: "all", inLibrary: true }, "", {
          key: "title",
          dir: "asc",
        }),
      ),
    ).toEqual(["4"]);
  });

  it("filters by status and searches titles, authors and series", () => {
    const sort = { key: "title", dir: "asc" } as const;
    expect(
      ids(tableRows(rows, { status: "to_read", inLibrary: false }, "", sort)),
    ).toEqual(["2"]);
    expect(ids(tableRows(rows, all, "zed", sort))).toEqual(["3"]);
    expect(ids(tableRows(rows, all, "saga", sort))).toEqual(["3"]);
  });

  it("puts books without the sorted value last either way", () => {
    expect(
      ids(tableRows(rows, all, "", { key: "rating", dir: "desc" })),
    ).toEqual(["3", "1", "2"]);
    expect(
      ids(tableRows(rows, all, "", { key: "rating", dir: "asc" })),
    ).toEqual(["1", "3", "2"]);
  });

  it("sorts by the date read, or added when unread", () => {
    expect(ids(tableRows(rows, all, "", { key: "date", dir: "desc" }))).toEqual(
      ["2", "1", "3"],
    );
  });
});
