/** The shape of a paperback's front cover, as an SVG filter can use it.
 *
 * Seen from the front, the cover rolls away around the spine and bows gently
 * towards the fore-edge. The rolled part is foreshortened and, being further
 * from the eye, looks shorter. `BookWarpFilter` turns this into a displacement
 * map, plus a shading map so the light follows the same surface.
 *
 * Positions are fractions of the cover: `u` runs from the spine (0) to the
 * fore-edge (1), `v` from top (0) to bottom (1). */

/** Where the cover stops rolling around the spine. */
const HINGE = 0.1;
/** Radius of the roll. A little over `HINGE`, so it stops short of edge-on. */
const ROLL_RADIUS = 0.115;
/** How far the roll's effect on height reaches. */
const SHOULDER = 0.3;
/** Height lost at the spine, and at the fore-edge. */
const SPINE_DROP = 0.02;
const FORE_EDGE_DROP = 0.01;
/** Where the cover sits closest to the eye. */
const CROWN = 0.4;

/** Distance along the cover's surface for a position seen from the front. */
function arcLength(u: number): number {
  return u >= HINGE
    ? u
    : HINGE - ROLL_RADIUS * Math.asin((HINGE - u) / ROLL_RADIUS);
}

/** The position in the flat artwork that shows at `u`. All of the artwork
 * stays on the cover: the roll squeezes it rather than cropping it. */
export function artworkAt(u: number): number {
  const start = arcLength(0);
  return (arcLength(u) - start) / (arcLength(1) - start);
}

/** The cover's apparent height at `u`, as a fraction of its full height. */
export function heightAt(u: number): number {
  const t = Math.min(u / SHOULDER, 1);
  const spine = SPINE_DROP * (1 - t * t * (3 - 2 * t));
  const bow = Math.max(u - CROWN, 0) / (1 - CROWN);
  return 1 - spine - FORE_EDGE_DROP * bow * bow;
}

/** The old flat gradient's crease and fore-edge glint, as `[u, light]` stops. */
const CREASE: readonly (readonly [number, number])[] = [
  [0, -0.22],
  [0.025, 0.18],
  [0.05, -0.12],
  [0.09, 0],
  [0.94, 0],
  [1, 0.06],
];

function crease(u: number): number {
  const next = CREASE.findIndex(([at]) => at >= u);
  if (next <= 0) return CREASE[0][1];
  const [a, from] = CREASE[next - 1];
  const [b, to] = CREASE[next];
  return from + ((to - from) * (u - a)) / (b - a);
}

/** Light falling on the cover at `u`: above zero brightens, below darkens. */
export function lightAt(u: number): number {
  const turned = u < HINGE ? (HINGE - u) / ROLL_RADIUS : 0;
  const rolledAway = 1 - Math.sqrt(1 - turned * turned);
  const sheen = 0.045 * Math.exp(-(((u - 0.32) / 0.22) ** 2));
  const falloff = 0.05 * Math.max(u - 0.72, 0) ** 2 * 12;
  return crease(u) - 0.2 * rolledAway + sheen - falloff;
}

/** The largest offset the warp needs, so the map spends its 8 bits well. */
function largestOffset(width: number): number {
  let largest = 0;
  for (let x = 0; x < width; x++) {
    const u = (x + 0.5) / width;
    largest = Math.max(
      largest,
      Math.abs(artworkAt(u) - u),
      // Top row: how far above the artwork this column's top edge lands.
      0.5 / heightAt(u) - 0.5,
    );
  }
  return largest;
}

export interface WarpMap {
  /** RGBA: red moves pixels sideways, green up and down. */
  pixels: Uint8ClampedArray<ArrayBuffer>;
  /** `feDisplacementMap`'s scale, as a fraction of the cover. */
  scale: number;
}

/** The displacement map: for each output pixel, where to read the artwork. */
export function warpMap(width: number, height: number): WarpMap {
  // Twice the offset because a channel covers ±scale/2, with room to round.
  const scale = largestOffset(width) * 2.02;
  const encode = (offset: number) => Math.round((offset / scale + 0.5) * 255);
  const pixels = new Uint8ClampedArray(width * height * 4);
  for (let x = 0; x < width; x++) {
    const u = (x + 0.5) / width;
    const across = encode(artworkAt(u) - u);
    const tall = heightAt(u);
    for (let y = 0; y < height; y++) {
      const v = (y + 0.5) / height;
      const i = (y * width + x) * 4;
      pixels[i] = across;
      pixels[i + 1] = encode(0.5 + (v - 0.5) / tall - v);
      pixels[i + 2] = 128;
      pixels[i + 3] = 255;
    }
  }
  return { pixels, scale };
}

/** The shading map: black or white, with `lightAt` as its opacity. */
export function shadeMap(width: number): Uint8ClampedArray<ArrayBuffer> {
  const pixels = new Uint8ClampedArray(width * 4);
  for (let x = 0; x < width; x++) {
    const light = lightAt((x + 0.5) / width);
    pixels.fill(light > 0 ? 255 : 0, x * 4, x * 4 + 3);
    pixels[x * 4 + 3] = Math.round(Math.abs(light) * 255);
  }
  return pixels;
}
