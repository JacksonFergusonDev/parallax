import { describe, expect, it } from "vitest";
import type { Book, DeviceBook } from "./api";
import { shortDate } from "./device";
import {
  deviceReviewGroups,
  formatSize,
  libraryReviewGroups,
} from "./duplicates";

const book = (id: number, extra: Partial<Book> = {}): Book => ({
  id,
  title: "Dune",
  authors: ["Frank Herbert"],
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
  original_name: "Dune.epub",
  file_size: 2_500_000,
  format: "epub",
  kind: "book",
  direction: "ltr",
  has_cover: false,
  custom_cover: false,
  cover_upscaled: false,
  added_at: "2024-01-03T00:00:00Z",
  modified_at: "2024-01-03T00:00:00Z",
  cover: null,
  ...extra,
});

const deviceBook = (id: string, extra: Partial<DeviceBook> = {}): DeviceBook =>
  ({
    id,
    title: "Dune",
    author: "Frank Herbert",
    path: id.replace("file:///mnt/onboard/", ""),
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
    cover: null,
    ...extra,
  }) as DeviceBook;

describe("formatSize", () => {
  it("uses KB below a megabyte", () => {
    expect(formatSize(200)).toBe("1 KB");
    expect(formatSize(300_000)).toBe("293 KB");
    expect(formatSize(2_500_000)).toBe("2.4 MB");
    expect(formatSize(null)).toBeNull();
  });
});

describe("libraryReviewGroups", () => {
  it("describes each copy and drops groups with unknown books", () => {
    const books = new Map([
      [1, book(1, { series: "Dune", series_index: "1", tags: ["Sci-Fi"] })],
      [2, book(2)],
    ]);
    const groups = libraryReviewGroups(
      [
        {
          key: "1\n2",
          reasons: ["same_title_and_author"],
          members: [
            { book_id: 1, remove: false },
            { book_id: 2, remove: true },
          ],
        },
        {
          key: "2\n9",
          reasons: ["same_isbn"],
          members: [
            { book_id: 2, remove: false },
            { book_id: 9, remove: true },
          ],
        },
      ],
      books,
      (id) => id === 1,
      () => null,
    );
    expect(groups).toHaveLength(1);
    expect(groups[0].members.map((m) => [m.id, m.remove])).toEqual([
      ["1", false],
      ["2", true],
    ]);
    expect(groups[0].members[0].facts).toEqual([
      "Dune #1",
      "EPUB · 2.4 MB",
      `Added ${shortDate("2024-01-03T00:00:00Z")}`,
      "Sci-Fi",
      "On Kobo",
    ]);
  });
});

describe("deviceReviewGroups", () => {
  it("locks store copies and shows progress", () => {
    const a = "file:///mnt/onboard/KEpub/Dune.kepub.epub";
    const books = new Map([
      [
        a,
        deviceBook(a, {
          read_status: "reading",
          percent_read: 40,
          last_read: "2024-01-03T00:00:00Z",
        }),
      ],
      ["store", deviceBook("store", { sideloaded: false, path: null })],
    ]);
    const [g] = deviceReviewGroups(
      [
        {
          key: "k",
          reasons: ["same_title_and_author"],
          members: [
            {
              content_id: a,
              format: "KEPUB",
              size: 1_048_576,
              removable: true,
              remove: false,
            },
            {
              content_id: "store",
              format: "Kobo store",
              size: null,
              removable: false,
              remove: false,
            },
          ],
        },
      ],
      books,
      () => null,
    );
    expect(g.members[0].facts).toEqual([
      "KEPUB · 1.0 MB",
      `40% · last read ${shortDate("2024-01-03T00:00:00Z")}`,
      "KEpub/Dune.kepub.epub",
    ]);
    expect(g.members[0].locked).toBeUndefined();
    expect(g.members[1].locked).toContain("Kobo store");
  });
});
