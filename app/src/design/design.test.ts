import { describe, expect, it } from "vitest";
import { paletteOf } from "../reader/appearance";
import { APP_THEMES, isDark, shownTheme, type ShownTheme } from "./theme";
// @ts-expect-error Node types are not part of the browser application.
import { readFileSync, readdirSync } from "node:fs";
const tokens: string = readFileSync(
  new URL("./tokens.css", import.meta.url),
  "utf8",
);

// Inspect every authored stylesheet so new screens cannot silently bypass the scale.
const sourceRoot = new URL("../", import.meta.url);
const styles = Object.fromEntries(
  (readdirSync(sourceRoot, { recursive: true }) as string[])
    .filter((path) => path.endsWith(".css"))
    .map((path) => [
      path,
      readFileSync(new URL(path, sourceRoot), "utf8") as string,
    ]),
);
const authoredFiles = (
  readdirSync(sourceRoot, { recursive: true }) as string[]
).filter(
  (path) =>
    /\.tsx?$/.test(path) &&
    !path.endsWith(".test.ts") &&
    !path.endsWith(".test.tsx"),
);
const artworkSelectors = new Set([
  ".gen-title",
  ".gen-author",
  ".gen-cover.mini .gen-title",
  ".panel-cover.placeholder .gen-title",
  ".panel-cover.placeholder .gen-author",
  ".now-cover .gen-cover.mini .gen-title",
]);

describe("Parallax design contract", () => {
  it("does not bypass the UI type scale with inline styles", () => {
    for (const path of authoredFiles.filter(
      (path) => !path.startsWith("reader/"),
    )) {
      const source = readFileSync(new URL(path, sourceRoot), "utf8");
      expect(source, path).not.toMatch(/fontSize\s*:/);
      expect(source, path).not.toMatch(/font-size\s*:/);
    }
  });

  it("keeps web and packaged brand assets identical", () => {
    const web = readFileSync(
      new URL("../../public/parallax.svg", import.meta.url),
      "utf8",
    );
    const packaged = readFileSync(
      new URL("../../src-tauri/icons/source/icon.svg", import.meta.url),
      "utf8",
    );
    expect(web).toBe(packaged);
  });

  for (const [path, css] of Object.entries(styles)) {
    if (path.endsWith("/tokens.css")) continue;
    it(`${path} uses the shared type scale and radii`, () => {
      const clean = css.replace(/\/\*[\s\S]*?\*\//g, "");
      for (const match of clean.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
        const [, selector, declarations] = match;
        for (const declaration of declarations.matchAll(
          /(?:^|;)\s*(font-size|font|border-radius)\s*:\s*([^;]+)/g,
        )) {
          const [, property, value] = declaration;
          if (property === "font-size") {
            const allowed =
              /^(inherit|var\(--text-(small|body|label|heading|display)\))$/;
            const artwork =
              artworkSelectors.has(selector.trim()) &&
              /^var\(--art-type-[\d-]+\)$/.test(value.trim());
            expect(
              allowed.test(value.trim()) || artwork,
              `${selector}: ${property}: ${value}`,
            ).toBe(true);
          }
          if (property === "font") expect(value.trim()).toBe("inherit");
          if (property === "border-radius") {
            // Pill shapes and book silhouettes are geometry, not control radii.
            expect(value).not.toMatch(/^(6|8|10|12|14)px$/);
          }
        }
      }
    });
  }

  it("keeps UI type between 13 and 24 logical pixels at default scale", () => {
    const values = [...tokens.matchAll(/--text-[\w-]+:\s*([\d.]+)rem/g)].map(
      (m) => Number(m[1]) * 14,
    );
    expect(values).toHaveLength(5);
    for (const value of values) {
      expect(value).toBeGreaterThanOrEqual(12.999);
      expect(value).toBeLessThanOrEqual(24.001);
    }
  });

  const colours = (block: string): Record<string, string> =>
    Object.fromEntries(
      [...block.matchAll(/--([\w-]+):\s*(#[a-f\d]{6})/g)].map((m) => [
        m[1],
        m[2],
      ]),
    );
  const themes = Object.fromEntries(
    [
      ...tokens.matchAll(
        /^(?::root,\s*)?\[data-theme="([\w-]+)"\]\s*\{([^}]+)\}/gm,
      ),
    ].map((m) => [m[1], colours(m[2])]),
  ) as Record<ShownTheme, Record<string, string>>;
  const shown = [
    ...new Set(
      APP_THEMES.flatMap(({ value }) => [
        shownTheme(value, false),
        shownTheme(value, true),
      ]),
    ),
  ];

  it("defines every colour role in every theme", () => {
    expect(Object.keys(themes).sort()).toEqual([...shown].sort());
    const roles = Object.keys(themes.light).sort();
    expect(roles).toHaveLength(12);
    for (const [name, palette] of Object.entries(themes))
      expect(Object.keys(palette).sort(), name).toEqual(roles);
    // Before a window picks its theme, the system's dark is the Dark theme.
    const fallback = tokens.match(/:root:not\(\[data-theme\]\)\s*\{([^}]+)\}/)!;
    expect(colours(fallback[1])).toEqual(themes.dark);
  });

  it("keeps reader palettes aligned with the shell", () => {
    for (const theme of shown) {
      const palette = themes[theme];
      expect(paletteOf(theme), theme).toEqual({
        background: isDark(theme) ? palette.bg : palette.surface,
        text: palette.fg,
        link: palette.accent,
      });
    }
  });

  it("maintains readable palette contrast in every theme", () => {
    const luminance = (hex: string) => {
      const channels = hex.match(/[a-f\d]{2}/gi)!.map((s) => {
        const n = parseInt(s, 16) / 255;
        return n <= 0.04045 ? n / 12.92 : ((n + 0.055) / 1.055) ** 2.4;
      });
      return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
    };
    const ratio = (a: string, b: string) => {
      const values = [luminance(a), luminance(b)].sort((x, y) => y - x);
      return (values[0] + 0.05) / (values[1] + 0.05);
    };
    for (const [name, palette] of Object.entries(themes)) {
      // High contrast means at least 7:1 for every colour of text.
      const least = name.startsWith("contrast") ? 7 : 4.5;
      for (const foreground of [
        "fg",
        "muted",
        "accent",
        "ok",
        "warn",
        "danger",
      ]) {
        for (const background of ["bg", "surface", "sunken"]) {
          expect(
            ratio(palette[foreground], palette[background]),
            `${name}: ${foreground} on ${background}`,
          ).toBeGreaterThanOrEqual(least);
        }
      }
      for (const status of ["ok", "warn", "danger"]) {
        expect(
          ratio(palette["status-fg"], palette[status]),
          `${name}: text on ${status}`,
        ).toBeGreaterThanOrEqual(least);
      }
      expect(
        ratio(palette["accent-fg"], palette.accent),
        `${name}: primary button text`,
      ).toBeGreaterThanOrEqual(least);
    }
  });
});
