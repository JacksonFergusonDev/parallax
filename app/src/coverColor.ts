// Sorting books by the dominant colour of their covers. The backend reads the
// colours; they're remembered here so each cover is only read once.

import { useCallback, useEffect, useMemo, useState } from "react";
import { coverColors } from "./api";
import { coverArt } from "./coverArt";
import type { Column, SortValue } from "./sort";

export type Rgb = [number, number, number];

/** A cover to read: `key` changes whenever the cover image does. */
export interface CoverRef {
  key: string;
  path: string;
}

const STORAGE_KEY = "cover-colors";
const BATCH = 100;
/** Below this spread between channels a colour reads as grey. */
const NEUTRAL_CHROMA = 0.12;

/** Hue in degrees and lightness from 0 to 1. */
function hueAndLightness([r, g, b]: Rgb): {
  hue: number;
  lightness: number;
  chroma: number;
} {
  const [x, y, z] = [r / 255, g / 255, b / 255];
  const max = Math.max(x, y, z);
  const min = Math.min(x, y, z);
  const chroma = max - min;
  const lightness = (max + min) / 2;
  if (chroma === 0) return { hue: 0, lightness, chroma };
  const sector =
    max === x
      ? ((y - z) / chroma + 6) % 6
      : max === y
        ? (z - x) / chroma + 2
        : (x - y) / chroma + 4;
  return { hue: sector * 60, lightness, chroma };
}

/**
 * A sort value that, descending, runs through the rainbow from red to violet,
 * light to dark within each hue band, then greys from white to black.
 */
export function colorSortValue(rgb: Rgb): SortValue {
  const { hue, lightness, chroma } = hueAndLightness(rgb);
  if (chroma < NEUTRAL_CHROMA) return [0, 0, lightness];
  // Shifted so the reds either side of 0° share a band.
  const band = Math.floor(((hue + 15) % 360) / 30);
  return [1, -band, lightness];
}

export function hexToRgb(hex: string): Rgb {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

/** The colour of the generated cover shown for a book without one. */
export const generatedColor = (title: string): Rgb =>
  hexToRgb(coverArt(title).palette.bg);

/** The "Colour" sort; it has no column of its own in the list view. */
export function colorColumn<T, K extends string>(
  key: K,
  colorOf: (item: T) => Rgb | null,
): Column<T, K> {
  return {
    key,
    label: "Colour",
    width: "0",
    value: (item) => {
      const rgb = colorOf(item);
      return rgb ? colorSortValue(rgb) : null;
    },
  };
}

function loadKnown(): Map<string, Rgb | null> {
  try {
    const saved = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "{}");
    return new Map(Object.entries(saved as Record<string, Rgb>));
  } catch {
    return new Map();
  }
}

/** Failures aren't saved, so those covers are tried again next launch. */
function saveKnown(known: Map<string, Rgb | null>) {
  try {
    const found = [...known].filter(([, rgb]) => rgb !== null);
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify(Object.fromEntries(found)),
    );
  } catch {
    /* storage unavailable */
  }
}

let knownColors: Map<string, Rgb | null> | null = null;
const known = () => (knownColors ??= loadKnown());

/** Reads the colours of `covers` while `enabled`, all at once when done, so
 * the shelf reorders a single time. */
export function useCoverColors(enabled: boolean, covers: CoverRef[]) {
  const [revision, setRevision] = useState(0);
  const missing = useMemo(
    () => (enabled ? covers.filter((c) => !known().has(c.key)) : []),
    // `revision` re-checks after new colours arrive.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [enabled, covers, revision],
  );

  useEffect(() => {
    if (missing.length === 0) return;
    let alive = true;
    (async () => {
      for (let i = 0; i < missing.length && alive; i += BATCH) {
        const batch = missing.slice(i, i + BATCH);
        const colors = await coverColors(batch.map((c) => c.path)).catch(() =>
          batch.map(() => null),
        );
        batch.forEach((c, n) => known().set(c.key, colors[n] ?? null));
      }
      saveKnown(known());
      if (alive) setRevision((r) => r + 1);
    })();
    return () => {
      alive = false;
    };
  }, [missing]);

  const colorOf = useCallback(
    (key: string) => known().get(key) ?? null,
    // A new function after each read, so sorted lists recompute.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [revision],
  );
  return { colorOf, loading: missing.length > 0 };
}
