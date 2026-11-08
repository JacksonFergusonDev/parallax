import { describe, expect, it } from "vitest";
import { appTheme, isDark, shownTheme } from "./theme";

describe("app themes", () => {
  it("reads unknown or missing settings as the system's", () => {
    expect(appTheme(null)).toBe("system");
    expect(appTheme("neon")).toBe("system");
    expect(appTheme("sepia")).toBe("sepia");
  });

  it("follows macOS only where the theme does", () => {
    expect(shownTheme("system", true)).toBe("dark");
    expect(shownTheme("system", false)).toBe("light");
    expect(shownTheme("contrast", true)).toBe("contrast-dark");
    expect(shownTheme("contrast", false)).toBe("contrast");
    expect(shownTheme("sepia", true)).toBe("sepia");
    expect(shownTheme("light", true)).toBe("light");
    expect(isDark("contrast-dark")).toBe(true);
    expect(isDark("sepia")).toBe(false);
  });
});
