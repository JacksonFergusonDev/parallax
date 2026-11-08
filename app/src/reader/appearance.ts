import {
  shownTheme as appShownTheme,
  type AppTheme,
  type ShownTheme,
} from "../design/theme";
import type { Settings, Theme } from "./backend";

export type { ShownTheme };

export const FONT_SIZES = [80, 90, 100, 110, 120, 135, 150, 175, 200];

interface Palette {
  background: string;
  text: string;
  link: string;
}

/** Each matches its app theme in tokens.css: the page is the surface in
 * light themes and the background in dark ones. */
const PALETTES: Record<ShownTheme, Palette> = {
  light: { background: "#fffdf8", text: "#29271f", link: "#856015" },
  sepia: { background: "#f1e4cc", text: "#43341f", link: "#8a3c1b" },
  dark: { background: "#191c21", text: "#f1ede4", link: "#f5c568" },
  contrast: { background: "#ffffff", text: "#000000", link: "#5c3f00" },
  "contrast-dark": { background: "#000000", text: "#ffffff", link: "#ffd479" },
};

/** Auto is whatever the app shows; High contrast follows macOS. */
export function shownTheme(
  theme: Theme,
  app: AppTheme,
  systemDark: boolean,
): ShownTheme {
  if (theme === "auto") return appShownTheme(app, systemDark);
  if (theme === "contrast") return appShownTheme("contrast", systemDark);
  return theme;
}

export const paletteOf = (theme: ShownTheme) => PALETTES[theme];

/** The next text size up or down, starting from the nearest one. */
export function stepFontSize(size: number, direction: 1 | -1): number {
  const nearest = FONT_SIZES.reduce(
    (best, s, i) =>
      Math.abs(s - size) < Math.abs(FONT_SIZES[best] - size) ? i : best,
    0,
  );
  const next = nearest + direction;
  return FONT_SIZES[Math.min(Math.max(next, 0), FONT_SIZES.length - 1)];
}

/** `settings` a text size up or down. */
export const resized = (settings: Settings, direction: 1 | -1): Settings => ({
  ...settings,
  fontSize: stepFontSize(settings.fontSize, direction),
});

const LINE_WIDTHS = { narrow: "520px", medium: "640px", wide: "820px" };
/** The space beside the text, as a share of the page. */
const MARGINS = { narrow: "3%", medium: "6%", wide: "10%" };
const LINE_HEIGHTS = { book: null, relaxed: 1.55, loose: 1.85 };

/** The engine's page layout, as attributes on its renderer. */
export function layoutAttributes(settings: Settings): Record<string, string> {
  return {
    flow: settings.flow,
    "max-inline-size": LINE_WIDTHS[settings.width],
    gap: MARGINS[settings.margins],
    "max-column-count": settings.spread === "single" ? "1" : "2",
  };
}

/**
 * CSS for the book's pages. Light keeps the publisher's colours; the others
 * override them, because dark text on a dark page is unreadable. Images keep
 * their own colours either way.
 */
export function bookStyles(settings: Settings, theme: ShownTheme): string {
  const { background, text, link } = PALETTES[theme];
  const size = `html { font-size: ${settings.fontSize}% !important; }`;
  const page = `html, body { background: ${background} !important; color: ${text}; }`;
  const lineHeight = LINE_HEIGHTS[settings.spacing];
  const spacing = lineHeight
    ? `\np, li, blockquote, dd, div { line-height: ${lineHeight} !important; }`
    : "";
  if (theme === "light") return `${size}\n${page}${spacing}`;
  return `${size}
${page}${spacing}
body, body :not(a, a *, img, svg, svg *, video) {
  color: ${text} !important;
  background-color: transparent !important;
  border-color: currentColor !important;
}
a, a * { color: ${link} !important; }`;
}
