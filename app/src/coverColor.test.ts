import { describe, expect, it } from "vitest";
import { colorColumn, colorSortValue, hexToRgb, type Rgb } from "./coverColor";
import { applySort } from "./sort";

const shelf = (colors: Record<string, Rgb | null>, dir: "asc" | "desc") => {
  const column = colorColumn<string, "color">("color", (n) => colors[n]);
  return applySort(Object.keys(colors), { key: "color", dir }, [column]);
};

describe("colour sort", () => {
  const colors: Record<string, Rgb | null> = {
    black: [10, 10, 10],
    blue: [30, 60, 200],
    unknown: null,
    white: [245, 245, 245],
    green: [40, 160, 70],
    red: [200, 30, 30],
    wineRed: [190, 20, 50],
    yellow: [230, 200, 40],
  };

  it("runs red to blue, then light to dark greys, unknown last", () => {
    expect(shelf(colors, "desc")).toEqual([
      "red",
      "wineRed",
      "yellow",
      "green",
      "blue",
      "white",
      "black",
      "unknown",
    ]);
  });

  it("reverses, still keeping unknown last", () => {
    const order = shelf(colors, "asc");
    expect(order[0]).toBe("black");
    expect(order.at(-1)).toBe("unknown");
  });

  it("treats a washed-out colour as grey", () => {
    expect(colorSortValue([128, 130, 134])).toEqual([0, 0, expect.any(Number)]);
  });
});

describe("hexToRgb", () => {
  it("reads a hex colour", () => {
    expect(hexToRgb("#b4532a")).toEqual([180, 83, 42]);
  });
});
