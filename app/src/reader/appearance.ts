import type { Settings, Theme } from "./backend";

export const FONT_SIZES = [80, 90, 100, 110, 120, 135, 150, 175, 200];

interface Palette {
  background: string;
  text: string;
  link: string;
}

const PALETTES: Record<Exclude<Theme, "auto">, Palette> = {
  light: { background: "#fffcf6", text: "#2a2118", link: "#b4532a" },
  sepia: { background: "#f1e4cc", text: "#43341f", link: "#94431f" },
  dark: { background: "#1b1612", text: "#e6dccd", link: "#e3895c" },
};

export type ShownTheme = keyof typeof PALETTES;

export const shownTheme = (theme: Theme, systemDark: boolean): ShownTheme =>
  theme === "auto" ? (systemDark ? "dark" : "light") : theme;

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

/**
 * CSS for the book's pages. Light keeps the publisher's colours; the others
 * override them, because dark text on a dark page is unreadable. Images keep
 * their own colours either way.
 */
export function bookStyles(settings: Settings, theme: ShownTheme): string {
  const { background, text, link } = PALETTES[theme];
  const size = `html { font-size: ${settings.fontSize}% !important; }`;
  const page = `html, body { background: ${background} !important; color: ${text}; }`;
  if (theme === "light") return `${size}\n${page}`;
  return `${size}
${page}
body, body :not(a, a *, img, svg, svg *, video) {
  color: ${text} !important;
  background-color: transparent !important;
  border-color: currentColor !important;
}
a, a * { color: ${link} !important; }`;
}
