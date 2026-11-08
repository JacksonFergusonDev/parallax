import { describe, expect, it } from "vitest";
import { bookStyles, shownTheme, stepFontSize } from "./appearance";

describe("text size", () => {
  it("moves one step at a time and stops at the ends", () => {
    expect(stepFontSize(100, 1)).toBe(110);
    expect(stepFontSize(100, -1)).toBe(90);
    expect(stepFontSize(200, 1)).toBe(200);
    expect(stepFontSize(80, -1)).toBe(80);
  });

  it("starts from the nearest step", () => {
    expect(stepFontSize(103, 1)).toBe(110);
  });
});

describe("themes", () => {
  it("follows the system on auto", () => {
    expect(shownTheme("auto", true)).toBe("dark");
    expect(shownTheme("auto", false)).toBe("light");
    expect(shownTheme("sepia", true)).toBe("sepia");
  });

  it("keeps the publisher's colours in light", () => {
    const css = bookStyles({ fontSize: 100, theme: "light" }, "light");
    expect(css).not.toContain("color: #29271f !important");
  });

  it("overrides text colour in dark, leaving images alone", () => {
    const css = bookStyles({ fontSize: 120, theme: "dark" }, "dark");
    expect(css).toContain("font-size: 120%");
    expect(css).toContain("color: #f1ede4 !important");
    expect(css).toContain(":not(a, a *, img, svg, svg *, video)");
  });
});
