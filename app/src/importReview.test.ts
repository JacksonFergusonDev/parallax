import { describe, expect, it } from "vitest";
import type {
  ImportResult,
  Issue,
  LibraryDuplicateGroup,
  Proposal,
  Review,
} from "./api";
import {
  addedIds,
  bestCover,
  coverIds,
  failedFiles,
  flaggedCount,
  manualIds,
  needsReview,
  reviewProposals,
  reviewSteps,
  summaryLines,
  type Lookup,
  type MeasuredCover,
  type ReviewBatch,
} from "./importReview";

const review = (
  findings: [Issue, number][],
  duplicates: LibraryDuplicateGroup[] = [],
): Review => ({
  findings: findings.map(([issue, book_id]) => ({
    issue,
    book_id,
    detail: "",
  })),
  duplicates,
  bulk: false,
});
const batch = (r: Review, failed: ReviewBatch["failed"] = []): ReviewBatch => ({
  ids: [1, 2],
  failed,
  review: r,
});
const proposal = (book_id: number, genre: string | null, series = false) =>
  ({
    book_id,
    series: series
      ? { name: "Saga", index: "1", source: "Wikidata", confident: true }
      : null,
    genres: genre
      ? [{ name: genre, sources: ["Open Library"], confident: true }]
      : [],
    problems: [],
  }) as Proposal;
const running: Lookup = { kind: "running", done: 0, total: 0, title: "" };
const found = (...proposals: Proposal[]): Lookup => ({
  kind: "done",
  proposals,
  problems: [],
});
const none = new Set<number>();
const pair: LibraryDuplicateGroup = {
  key: "k",
  reasons: ["same_isbn"],
  members: [
    { book_id: 1, remove: false },
    { book_id: 9, remove: true },
  ],
};
const cover = (height: number): MeasuredCover => ({
  candidate: { source: "Apple Books", url: `u${height}`, preview_url: "p" },
  width: Math.round(height * 0.66),
  height,
});

describe("what import results count", () => {
  const results = [
    {
      path: "/a/one.epub",
      outcome: {
        Ok: { kind: "added", book: { id: 1 }, possible_duplicate_of: null },
      },
    },
    {
      path: "/a/two.epub",
      outcome: { Ok: { kind: "already_in_library", book: { id: 2 } } },
    },
    { path: "/a/three.epub", outcome: { Err: "not a valid EPUB" } },
  ] as ImportResult[];

  it("treats only new books as added", () => {
    expect(addedIds(results)).toEqual([1]);
  });

  it("names failed files with why", () => {
    expect(failedFiles(results)).toEqual([
      { name: "three.epub", reason: "not a valid EPUB" },
    ]);
  });
});

describe("needsReview", () => {
  it("stays quiet for clean books", () => {
    expect(needsReview(batch(review([])))).toBe(false);
  });

  it("doesn't open for a low-resolution cover alone", () => {
    expect(needsReview(batch(review([["small_cover", 1]])))).toBe(false);
  });

  it("opens for anything else", () => {
    expect(needsReview(batch(review([["no_genre", 1]])))).toBe(true);
    expect(needsReview(batch(review([["no_cover", 1]])))).toBe(true);
    expect(needsReview(batch(review([], [pair])))).toBe(true);
    expect(
      needsReview(batch(review([]), [{ name: "a.epub", reason: "bad" }])),
    ).toBe(true);
  });
});

it("counts each flagged book once", () => {
  const r = review(
    [
      ["no_genre", 1],
      ["no_cover", 1],
      ["no_genre", 2],
    ],
    [pair],
  );
  expect(flaggedCount(batch(r))).toBe(3);
});

describe("reviewProposals", () => {
  it("keeps a series but not extra genres for a book that has a genre", () => {
    const r = review([["no_genre", 1]]);
    const out = reviewProposals(
      [
        proposal(1, "Fantasy"),
        proposal(2, "History", true),
        proposal(3, "War"),
      ],
      r,
    );
    expect(out.map((p) => [p.book_id, p.genres.length])).toEqual([
      [1, 1],
      [2, 0],
    ]);
  });

  it("drops removed books", () => {
    const r = review([["no_genre", 1]]);
    expect(reviewProposals([proposal(1, "Fantasy")], r, new Set([1]))).toEqual(
      [],
    );
  });
});

describe("bestCover", () => {
  it("takes the tallest for a book with no cover", () => {
    expect(bestCover([cover(400), cover(800)], null)?.height).toBe(800);
  });

  it("needs a sharp, taller cover to replace one", () => {
    expect(bestCover([cover(1100)], 800)).toBeNull();
    expect(bestCover([cover(1400)], 1500)).toBeNull();
    expect(bestCover([cover(1400)], 800)?.height).toBe(1400);
  });

  it("finds nothing among no covers", () => {
    expect(bestCover([], null)).toBeNull();
  });
});

describe("manualIds", () => {
  const r = review([
    ["no_genre", 1],
    ["no_genre", 2],
    ["no_author", 3],
  ]);

  it("waits for the lookup before asking for genres", () => {
    expect(manualIds(r, running)).toEqual([3]);
  });

  it("asks only for genres nothing proposed", () => {
    expect(manualIds(r, found(proposal(1, "Fantasy")))).toEqual([3, 2]);
  });

  it("asks for genres when the lookup failed", () => {
    expect(manualIds(r, { kind: "failed", message: "offline" })).toEqual([
      3, 1, 2,
    ]);
  });
});

describe("reviewSteps", () => {
  it("keeps only steps with a decision, in order", () => {
    const r = review([["small_cover", 1]], [pair]);
    expect(reviewSteps(batch(r), found(), none, true)).toEqual([
      "duplicates",
      "covers",
    ]);
  });

  it("expects details while lookups run, then drops them if nothing came", () => {
    const r = review([]);
    expect(reviewSteps(batch(r), running, none, null)).toEqual(["metadata"]);
    expect(reviewSteps(batch(r), found(), none, null)).toEqual([]);
  });

  it("includes a series found for a book with no findings", () => {
    const lookup = found(proposal(1, null, true));
    expect(reviewSteps(batch(review([])), lookup, none, null)).toEqual([
      "metadata",
    ]);
  });

  it("drops covers when there's nothing to offer", () => {
    const r = review([["no_cover", 1]]);
    expect(reviewSteps(batch(r), found(), none, null)).toEqual(["covers"]);
    expect(reviewSteps(batch(r), found(), none, false)).toEqual([]);
  });

  it("forgets removed books", () => {
    const r = review([["no_cover", 1]]);
    expect(coverIds(r, new Set([1]))).toEqual([]);
    expect(reviewSteps(batch(r), found(), new Set([1]), true)).toEqual([]);
  });
});

describe("summaryLines", () => {
  const title = (id: number) => `Book ${id}`;
  const keys = (b: ReviewBatch, lookup: Lookup) =>
    summaryLines(b, lookup, none, null, title).map((l) => l.key);

  it("leads with what can't be fixed here", () => {
    const r = review(
      [
        ["no_cover", 1],
        ["no_genre", 1],
        ["drm", 2],
        ["damaged", 1],
      ],
      [pair],
    );
    const b = batch(r, [{ name: "a.epub", reason: "bad" }]);
    expect(keys(b, running)).toEqual([
      "failed",
      "damaged",
      "drm",
      "duplicates",
      "genres",
      "covers",
    ]);
  });

  it("offers to remove what can't be fixed", () => {
    const lines = summaryLines(
      batch(review([["damaged", 1]])),
      running,
      none,
      null,
      title,
    );
    expect(lines[0].removable).toEqual([1]);
    expect(lines[0].warning).toBe(true);
    expect(lines[0].advice).toContain("Book 1");
  });

  it("follows the lookup", () => {
    const b = batch(
      review([
        ["no_genre", 1],
        ["no_genre", 2],
      ]),
    );
    const advice = (lookup: Lookup) =>
      summaryLines(b, lookup, none, null, title)[0].advice;
    expect(advice({ ...running, done: 1, total: 2 } as Lookup)).toContain(
      "1 of 2",
    );
    expect(advice(found(proposal(1, "Fantasy")))).toContain(
      "Found genres for 1",
    );
    expect(advice(found())).toContain("Nothing found");
  });

  it("mentions a series it wasn't asked for", () => {
    const lookup = found(proposal(1, null, true));
    expect(keys(batch(review([])), lookup)).toEqual(["series"]);
  });

  it("uses singular and plural", () => {
    const line = (r: Review) =>
      summaryLines(batch(r), running, none, null, title)[0].text;
    expect(line(review([["no_author", 1]]))).toBe("1 book has no author");
    expect(
      line(
        review([
          ["no_author", 1],
          ["no_author", 2],
        ]),
      ),
    ).toBe("2 books have no author");
  });
});
