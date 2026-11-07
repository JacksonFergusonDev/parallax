/** The shape of a paperback's front cover, as an SVG filter can use it.
 *
 * Seen from the front, the cover rolls away around the spine, which squeezes
 * the art there. Only the art moves: a book's outline is a clean rectangle, so
 * the top and bottom edges stay straight. `BookWarpFilter` turns this into a
 * displacement map, plus a shading map so the light follows the same surface.
 *
 * Positions are fractions of the cover: `u` runs from the spine (0) to the
 * fore-edge (1). */

/** Where the cover stops rolling around the spine. */
const HINGE = 0.1;
/** Radius of the roll. A little over `HINGE`, so it stops short of edge-on. */
const ROLL_RADIUS = 0.115;

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

export interface WarpMap {
  /** RGBA: red moves pixels sideways; the other channels stay neutral. */
  pixels: Uint8ClampedArray<ArrayBuffer>;
  /** `feDisplacementMap`'s scale, as a fraction of the cover. */
  scale: number;
}

/** The displacement map: for each output pixel, where to read the artwork. */
export function warpMap(width: number, height: number): WarpMap {
  const offsets = Array.from({ length: width }, (_, x) => {
    const u = (x + 0.5) / width;
    return artworkAt(u) - u;
  });
  // A channel covers ±scale/2, so the largest offset needs twice that.
  const scale = Math.max(...offsets.map(Math.abs)) * 2.02;
  const pixels = new Uint8ClampedArray(width * height * 4);
  for (let i = 0; i < pixels.length; i += 4) {
    const offset = offsets[(i / 4) % width];
    pixels[i] = Math.round((offset / scale + 0.5) * 255);
    pixels[i + 1] = 128;
    pixels[i + 2] = 128;
    pixels[i + 3] = 255;
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
