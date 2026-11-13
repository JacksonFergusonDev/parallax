import { describe, expect, it } from "vitest";
import type { GoodreadsPlan, GoodreadsReport } from "./api";
import { goodreadsSummary } from "./goodreads";

const plan = (p: Partial<GoodreadsPlan>): GoodreadsPlan => ({
  summary: {
    books: 0,
    read: 0,
    to_read: 0,
    currently_reading: 0,
    shelves: [],
    in_library: 0,
  },
  added: 0,
  updated: 0,
  unchanged: 0,
  skipped: 0,
  conflicts: [],
  missing: [],
  ...p,
});
const report = (r: Partial<GoodreadsReport>): GoodreadsReport => ({
  added: 0,
  updated: 0,
  unchanged: 0,
  skipped: 0,
  kept: 0,
  removed: 0,
  kept_yours: 0,
  took_goodreads: 0,
  ...r,
});

describe("goodreadsSummary", () => {
  it("says what will happen before importing", () => {
    expect(goodreadsSummary(plan({ added: 283 }))).toBe(
      "283 books will be added.",
    );
    expect(
      goodreadsSummary(plan({ added: 1, updated: 2, unchanged: 280 })),
    ).toBe(
      "1 book will be added, 2 books will be updated and 280 already up to date.",
    );
  });

  it("says what happened after importing, without hedging", () => {
    expect(
      goodreadsSummary(
        report({ updated: 1, kept: 2, removed: 1, unchanged: 5 }),
      ),
    ).toBe(
      "1 book updated, 2 books kept as your own, 1 book removed and 5 already up to date.",
    );
  });

  it("says when there's nothing to do and mentions skipped rows", () => {
    expect(goodreadsSummary(plan({}))).toBe("Nothing to change.");
    expect(goodreadsSummary(plan({ unchanged: 3, skipped: 2 }))).toBe(
      "3 already up to date. 2 rows skipped because Goodreads gave no book id.",
    );
  });
});
