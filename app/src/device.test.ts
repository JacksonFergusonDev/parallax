import { describe, expect, it } from "vitest";
import type { DeviceBook, DeviceImportOutcome } from "./api";
import {
  deviceColumns,
  filterDeviceBooks,
  shortDate,
  statusLabel,
  summarize,
  summarizeDeviceImport,
} from "./device";
import { applySort } from "./sort";

const book = (title: string, extra: Partial<DeviceBook> = {}): DeviceBook => ({
  id: `file:///mnt/onboard/KEpub/${title}.kepub.epub`,
  title,
  author: null,
  path: `KEpub/${title}.kepub.epub`,
  sideloaded: true,
  series: null,
  series_number: null,
  read_status: "unread",
  percent_read: 0,
  last_read: null,
  reading_time_secs: null,
  added: null,
  file_size: null,
  collections: [],
  ...extra,
});

describe("device helpers", () => {
  const books = [
    book("Dune", {
      author: "Frank Herbert",
      read_status: "reading",
      percent_read: 42,
    }),
    book("Emma", {
      collections: ["Classics"],
      read_status: "finished",
      percent_read: 100,
    }),
    book("Warbreaker"),
  ];

  it("filters across title, author, series and collections", () => {
    expect(filterDeviceBooks(books, "herbert").map((b) => b.title)).toEqual([
      "Dune",
    ]);
    expect(filterDeviceBooks(books, "classics").map((b) => b.title)).toEqual([
      "Emma",
    ]);
  });

  it("labels status and summarises", () => {
    expect(books.map(statusLabel)).toEqual(["42%", "Finished", "Unread"]);
    expect(summarize(books)).toBe("3 books · 1 in progress · 1 finished");
  });

  it("formats dates relative to now", () => {
    const now = new Date("2026-10-31T12:00:00Z");
    expect(shortDate(null, now)).toBe("—");
    expect(shortDate("2026-10-31T08:00:00Z", now)).toBe("Today");
    expect(shortDate("2026-10-30T08:00:00Z", now)).toBe("Yesterday");
    expect(shortDate("2026-10-27T08:00:00Z", now)).toBe("4 days ago");
  });
});

describe("device sorting", () => {
  it("sorts status by progress, finished first and unread last", () => {
    const books = [
      book("A"),
      book("B", { read_status: "reading", percent_read: 40 }),
      book("C", { read_status: "finished", percent_read: 100 }),
      book("D", { read_status: "reading", percent_read: 75 }),
    ];
    const titles = (dir: "asc" | "desc") =>
      applySort(books, { key: "status", dir }, deviceColumns).map(
        (b) => b.title,
      );
    expect(titles("desc")).toEqual(["C", "D", "B", "A"]);
    expect(titles("asc")).toEqual(["A", "B", "D", "C"]);
  });
  it("keeps never-read books last in both directions", () => {
    const books = [
      book("A"),
      book("B", { last_read: "2026-10-01T00:00:00Z" }),
      book("C", { last_read: "2026-10-05T00:00:00Z" }),
    ];
    const titles = (dir: "asc" | "desc") =>
      applySort(books, { key: "last_read", dir }, deviceColumns).map(
        (b) => b.title,
      );
    expect(titles("desc")).toEqual(["C", "B", "A"]);
    expect(titles("asc")).toEqual(["B", "C", "A"]);
  });
});

describe("summarizeDeviceImport", () => {
  const o = (result: DeviceImportOutcome["result"]): DeviceImportOutcome => ({
    content_id: "x",
    title: "x",
    result,
  });
  it("counts each outcome and groups skip reasons", () => {
    expect(
      summarizeDeviceImport([
        o({ Ok: { kind: "added", book_id: 1 } }),
        o({ Ok: { kind: "added", book_id: 2 } }),
        o({ Ok: { kind: "linked", book_id: 3 } }),
        o({ Ok: { kind: "skipped", reason: "DRM-protected" } }),
        o({ Ok: { kind: "skipped", reason: "DRM-protected" } }),
        o({ Err: "bad zip" }),
      ]),
    ).toBe(
      "Added 2 books to your library · matched 1 book you already had · 2 books skipped: DRM-protected · 1 book couldn't be read",
    );
    expect(summarizeDeviceImport([])).toBe("Nothing to add");
  });
});
