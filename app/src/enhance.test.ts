import { describe, expect, it } from "vitest";
import type { Proposal } from "./api";
import {
  countChanges,
  initialSelection,
  selectAll,
  toAccepted,
  toggleGenre,
  toggleSeries,
  withChanges,
} from "./enhance";

const proposals: Proposal[] = [
  {
    book_id: 1,
    series: {
      name: "Discworld",
      index: "1",
      source: "Wikidata",
      confident: true,
    },
    genres: [
      { name: "Fantasy", sources: ["Open Library"], confident: true },
      { name: "Humor", sources: ["Apple Books"], confident: false },
    ],
    problems: [],
  },
  {
    book_id: 2,
    series: {
      name: "Dover Thrift Editions",
      index: null,
      source: "Wikidata",
      confident: false,
    },
    genres: [],
    problems: ["Google Books: rate limited"],
  },
  { book_id: 3, series: null, genres: [], problems: [] },
];

describe("enhance selection", () => {
  it("ticks only confident proposals", () => {
    const sel = initialSelection(proposals);
    expect(countChanges(sel)).toBe(2);
    expect(toAccepted(proposals, sel)).toEqual([
      { book_id: 1, series: ["Discworld", "1"], genres: ["Fantasy"] },
    ]);
  });

  it("toggles single changes", () => {
    let sel = initialSelection(proposals);
    sel = toggleGenre(sel, 1, "Humor");
    sel = toggleSeries(sel, 1);
    sel = toggleSeries(sel, 2);
    expect(toAccepted(proposals, sel)).toEqual([
      { book_id: 1, series: null, genres: ["Fantasy", "Humor"] },
      { book_id: 2, series: ["Dover Thrift Editions", null], genres: [] },
    ]);
  });

  it("selects all or none", () => {
    expect(countChanges(selectAll(proposals, true))).toBe(4);
    expect(toAccepted(proposals, selectAll(proposals, false))).toEqual([]);
  });

  it("hides books with nothing to change", () => {
    expect(withChanges(proposals).map((p) => p.book_id)).toEqual([1, 2]);
  });
});
