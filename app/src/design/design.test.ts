import { describe, expect, it } from "vitest";
import { paletteOf } from "../reader/appearance";
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

  it("keeps reader light and dark palettes aligned with the shell", () => {
    const blocks = [...tokens.matchAll(/:root\s*\{([^}]+)\}/g)];
    for (const [index, theme] of (["light", "dark"] as const).entries()) {
      const palette = Object.fromEntries(
        [...blocks[index][1].matchAll(/--([\w-]+):\s*(#[a-f\d]{6})/g)].map(
          (m) => [m[1], m[2]],
        ),
      );
      expect(paletteOf(theme)).toEqual({
        background: theme === "light" ? palette.surface : palette.bg,
        text: palette.fg,
        link: palette.accent,
      });
    }
  });

  it("maintains readable palette contrast in both appearances", () => {
    const luminance = (hex: string) => {
      const channels = hex.match(/[a-f\d]{2}/gi)!.map((s) => {
        const n = parseInt(s, 16) / 255;
        return n <= 0.04045 ? n / 12.92 : ((n + 0.055) / 1.055) ** 2.4;
      });
      return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
    };
    expect([...tokens.matchAll(/:root\s*\{/g)]).toHaveLength(2);
    for (const block of tokens.matchAll(/:root\s*\{([^}]+)\}/g)) {
      const palette = Object.fromEntries(
        [...block[1].matchAll(/--([\w-]+):\s*(#[a-f\d]{6})/g)].map((m) => [
          m[1],
          m[2],
        ]),
      );
      for (const foreground of [
        "fg",
        "muted",
        "accent",
        "ok",
        "warn",
        "danger",
      ]) {
        for (const background of ["bg", "surface", "sunken"]) {
          const values = [
            luminance(palette[foreground]),
            luminance(palette[background]),
          ].sort((a, b) => b - a);
          expect(
            (values[0] + 0.05) / (values[1] + 0.05),
            `${foreground} on ${background}`,
          ).toBeGreaterThanOrEqual(4.5);
        }
      }
      for (const status of ["ok", "warn", "danger"]) {
        const values = [
          luminance(palette["status-fg"]),
          luminance(palette[status]),
        ].sort((a, b) => b - a);
        expect(
          (values[0] + 0.05) / (values[1] + 0.05),
          `text on ${status}`,
        ).toBeGreaterThanOrEqual(4.5);
      }
      const values = [
        luminance(palette["accent-fg"]),
        luminance(palette.accent),
      ].sort((a, b) => b - a);
      expect((values[0] + 0.05) / (values[1] + 0.05)).toBeGreaterThanOrEqual(
        4.5,
      );
    }
  });
});
