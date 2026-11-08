import { describe, expect, it } from "vitest";
import { chapterMarks, duration, whereabouts } from "./progress";

describe("time left", () => {
  it("reads naturally", () => {
    expect(duration(0.2)).toBe("less than a minute");
    expect(duration(4.4)).toBe("4 min");
    expect(duration(60)).toBe("1 h");
    expect(duration(201)).toBe("3 h 21 min");
  });

  it("shows what the engine knows and nothing it doesn't", () => {
    expect(
      whereabouts({
        cfi: "x",
        fraction: 0.5,
        pageItem: { label: " 214 ", href: "p.xhtml#p214" },
        time: { section: 12, total: 190 },
      }),
    ).toEqual({
      page: "p. 214",
      chapterLeft: "12 min left in chapter",
      bookLeft: "About 3 h 10 min left in the book",
    });
    expect(whereabouts({ cfi: "x", fraction: 0 })).toEqual({
      page: null,
      chapterLeft: null,
      bookLeft: null,
    });
    expect(
      whereabouts({ cfi: "x", fraction: 0, time: { section: NaN, total: 3 } })
        .chapterLeft,
    ).toBeNull();
  });
});

describe("chapter marks", () => {
  const fractions = [0, 0.1, 0.105, 0.4, 0.8, 1];

  it("marks where each chapter starts, in order", () => {
    expect(chapterMarks([3, 1, 4], fractions)).toEqual([0.1, 0.4, 0.8]);
  });

  it("leaves out the ends, repeats and marks too close together", () => {
    expect(chapterMarks([0, 1, 1, 2, 5, 9], fractions)).toEqual([0.1]);
  });
});
