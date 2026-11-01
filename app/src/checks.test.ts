import { describe, expect, it } from "vitest";
import { groupFindings } from "./checks";

describe("groupFindings", () => {
  it("puts problems before notes in a fixed order", () => {
    const groups = groupFindings([
      { check: "embedded_fonts", book_ids: [1], detail: "" },
      { check: "small_cover", book_ids: [2], detail: "500×750" },
      { check: "series_gap", book_ids: [3, 4], detail: "X: #2 missing" },
      { check: "drm", book_ids: [5], detail: "" },
      { check: "small_cover", book_ids: [6], detail: "600×900" },
    ]);
    expect(groups.map((g) => [g.kind, g.findings.length])).toEqual([
      ["drm", 1],
      ["small_cover", 2],
      ["series_gap", 1],
      ["embedded_fonts", 1],
    ]);
  });
});
