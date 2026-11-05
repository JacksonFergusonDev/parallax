/** Designed stand-in covers for books without one: a palette and a motif
 * picked deterministically from the title, so a book always looks the same. */

export interface CoverPalette {
  /** Cover background. */
  bg: string;
  /** Title and motif colour. */
  ink: string;
  /** Secondary detail colour (author, rules). */
  accent: string;
}

/** Hand-picked pairs in the spirit of classic paperback series. */
export const PALETTES: readonly CoverPalette[] = [
  { bg: "#b4532a", ink: "#fbf1e4", accent: "#f2c48d" }, // terracotta
  { bg: "#264653", ink: "#f4ecdc", accent: "#e9c46a" }, // deep teal
  { bg: "#e9c46a", ink: "#2a2118", accent: "#8a3b12" }, // mustard
  { bg: "#3d5a3f", ink: "#f2ecd9", accent: "#d8b26e" }, // forest
  { bg: "#6b2d3c", ink: "#f7e6dc", accent: "#e8a598" }, // oxblood
  { bg: "#1f2a44", ink: "#efe7d6", accent: "#c9a96e" }, // navy
  { bg: "#d8cbb3", ink: "#2f2419", accent: "#a2452a" }, // bone
  { bg: "#7a8b6f", ink: "#fbf6ea", accent: "#2f3a29" }, // sage
  { bg: "#c06c4b", ink: "#1f1611", accent: "#f6dcc1" }, // clay
  { bg: "#4a3b5c", ink: "#f1e8f0", accent: "#d9b8c4" }, // plum
];

export const MOTIFS = ["band", "sun", "stripes", "frame", "arch"] as const;
export type Motif = (typeof MOTIFS)[number];

/** FNV-1a: small, fast and stable across runs and platforms. */
export function hashString(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

export function coverArt(title: string): {
  palette: CoverPalette;
  motif: Motif;
} {
  const h = hashString(title.trim().toLowerCase());
  return {
    palette: PALETTES[h % PALETTES.length],
    motif: MOTIFS[Math.floor(h / PALETTES.length) % MOTIFS.length],
  };
}

/** A spine colour for the n-th book in a stack, varied but repeatable. */
export function spineColor(seed: string): string {
  return PALETTES[hashString(seed) % PALETTES.length].bg;
}
