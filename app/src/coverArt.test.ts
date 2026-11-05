import { describe, expect, it } from "vitest";
import { MOTIFS, PALETTES, coverArt, hashString, spineColor } from "./coverArt";

describe("coverArt", () => {
  it("is stable for the same title, ignoring case and outer spaces", () => {
    expect(coverArt("Dune")).toEqual(coverArt("  dune "));
  });

  it("spreads titles across palettes and motifs", () => {
    const titles = Array.from({ length: 200 }, (_, i) => `Book ${i}`);
    const palettes = new Set(titles.map((t) => coverArt(t).palette));
    const motifs = new Set(titles.map((t) => coverArt(t).motif));
    expect(palettes.size).toBe(PALETTES.length);
    expect(motifs.size).toBe(MOTIFS.length);
  });

  it("hashes to an unsigned 32-bit integer", () => {
    for (const s of ["", "a", "The Lies of Locke Lamora"]) {
      const h = hashString(s);
      expect(Number.isInteger(h)).toBe(true);
      expect(h).toBeGreaterThanOrEqual(0);
      expect(h).toBeLessThan(2 ** 32);
    }
  });

  it("picks spine colours from the palette", () => {
    const bgs = PALETTES.map((p) => p.bg);
    expect(bgs).toContain(spineColor("2023-4"));
  });
});
