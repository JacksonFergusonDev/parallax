import { describe, expect, it } from "vitest";
import {
  bookStyles,
  layoutAttributes,
  shownTheme,
  stepFontSize,
} from "./appearance";
import { DEFAULT_SETTINGS } from "./backend";

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
    const css = bookStyles({ ...DEFAULT_SETTINGS, theme: "light" }, "light");
    expect(css).not.toContain("color: #29271f !important");
  });

  it("overrides text colour in dark, leaving images alone", () => {
    const css = bookStyles(
      { ...DEFAULT_SETTINGS, fontSize: 120, theme: "dark" },
      "dark",
    );
    expect(css).toContain("font-size: 120%");
    expect(css).toContain("color: #f1ede4 !important");
    expect(css).toContain(":not(a, a *, img, svg, svg *, video)");
  });
});

describe("layout", () => {
  it("keeps the publisher's line height unless asked", () => {
    expect(bookStyles(DEFAULT_SETTINGS, "light")).not.toContain("line-height");
    const loose = bookStyles({ ...DEFAULT_SETTINGS, spacing: "loose" }, "dark");
    expect(loose).toContain("line-height: 1.85 !important");
  });

  it("turns settings into the engine's attributes", () => {
    expect(layoutAttributes(DEFAULT_SETTINGS)).toEqual({
      flow: "paginated",
      "max-inline-size": "640px",
      gap: "6%",
      "max-column-count": "2",
    });
    const scrolled = layoutAttributes({
      ...DEFAULT_SETTINGS,
      flow: "scrolled",
      width: "wide",
      margins: "narrow",
      spread: "single",
    });
    expect(scrolled).toEqual({
      flow: "scrolled",
      "max-inline-size": "820px",
      gap: "3%",
      "max-column-count": "1",
    });
  });
});
