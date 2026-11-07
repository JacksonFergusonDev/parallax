import { describe, expect, it } from "vitest";
import { artworkAt, heightAt, lightAt, shadeMap, warpMap } from "./bookWarp";

const steps = Array.from({ length: 101 }, (_, i) => i / 100);

describe("artworkAt", () => {
  it("keeps all of the artwork on the cover", () => {
    expect(artworkAt(0)).toBeCloseTo(0);
    expect(artworkAt(1)).toBeCloseTo(1);
  });

  it("never folds the artwork back on itself", () => {
    for (let i = 1; i < steps.length; i++) {
      expect(artworkAt(steps[i])).toBeGreaterThan(artworkAt(steps[i - 1]));
    }
  });

  it("squeezes the artwork where the cover rolls away", () => {
    const atSpine = artworkAt(0.01) - artworkAt(0);
    const onTheFlat = artworkAt(0.51) - artworkAt(0.5);
    expect(atSpine).toBeGreaterThan(onTheFlat * 1.5);
  });
});

describe("heightAt", () => {
  it("is tallest at the crown and shorter towards both edges", () => {
    expect(heightAt(0.4)).toBeCloseTo(1);
    expect(heightAt(0)).toBeLessThan(heightAt(1));
    expect(heightAt(1)).toBeLessThan(1);
  });

  it("curves without a visible kink", () => {
    for (let i = 2; i < steps.length; i++) {
      const bend =
        heightAt(steps[i]) -
        2 * heightAt(steps[i - 1]) +
        heightAt(steps[i - 2]);
      expect(Math.abs(bend)).toBeLessThan(0.0005);
    }
  });
});

describe("lightAt", () => {
  it("darkens the spine and catches the crease's ridge", () => {
    expect(lightAt(0)).toBeLessThan(-0.3);
    expect(lightAt(0.025)).toBeGreaterThan(0);
  });

  it("stays subtle across the face of the cover", () => {
    for (const u of steps.filter((u) => u > 0.1)) {
      expect(Math.abs(lightAt(u))).toBeLessThan(0.1);
    }
  });
});

describe("warpMap", () => {
  const width = 64;
  const height = 96;
  const { pixels, scale } = warpMap(width, height);
  const decode = (channel: number) => (channel / 255 - 0.5) * scale;
  const at = (x: number, y: number) => (y * width + x) * 4;

  it("sends each pixel to its place in the artwork", () => {
    const x = 3;
    const u = (x + 0.5) / width;
    expect(u + decode(pixels[at(x, 40)])).toBeCloseTo(artworkAt(u), 2);
  });

  it("reads past the artwork above the top edge, leaving it transparent", () => {
    const v = 0.5 / height;
    expect(v + decode(pixels[at(0, 0) + 1])).toBeLessThan(0);
  });

  it("leaves the middle row where it is", () => {
    const lower = decode(pixels[at(0, height / 2) + 1]);
    const upper = decode(pixels[at(0, height / 2 - 1) + 1]);
    expect(Math.abs(lower + upper)).toBeLessThan(scale / 100);
  });

  it("fits every offset inside the channel", () => {
    const channels = pixels.filter((_, i) => i % 4 < 2);
    expect(Math.min(...channels)).toBeGreaterThan(0);
    expect(Math.max(...channels)).toBeLessThan(255);
  });
});

describe("shadeMap", () => {
  it("is black at the spine and white on the ridge", () => {
    const pixels = shadeMap(100);
    expect(pixels[0]).toBe(0);
    expect(pixels[2 * 4]).toBe(255);
    expect(pixels[3]).toBeGreaterThan(40);
  });
});
