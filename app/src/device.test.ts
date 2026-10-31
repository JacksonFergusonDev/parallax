import { describe, expect, it } from "vitest";
import type { DeviceBook } from "./api";
import { filterDeviceBooks, shortDate, statusLabel, summarize } from "./device";

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
