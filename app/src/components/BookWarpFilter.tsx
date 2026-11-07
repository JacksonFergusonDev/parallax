import { useState } from "react";
import { shadeMap, warpMap } from "../bookWarp";

const MAP_WIDTH = 256;
const MAP_HEIGHT = 384;

function pngOf(
  pixels: Uint8ClampedArray<ArrayBuffer>,
  width: number,
  height: number,
): string | null {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) return null;
  context.putImageData(new ImageData(pixels, width, height), 0, 0);
  return canvas.toDataURL("image/png");
}

function buildMaps() {
  const { pixels, scale } = warpMap(MAP_WIDTH, MAP_HEIGHT);
  const warp = pngOf(pixels, MAP_WIDTH, MAP_HEIGHT);
  const shade = pngOf(shadeMap(MAP_WIDTH), MAP_WIDTH, 1);
  return warp && shade ? { warp, shade, scale } : null;
}

/** Defines `#book-warp`, which rolls a flat cover's art around the spine
 * and shades it. One filter serves images and generated covers alike; covers
 * pick it up in styles.css. Without a canvas, covers simply stay flat. */
export function BookWarpFilter() {
  const [maps] = useState(buildMaps);
  if (!maps) return null;
  return (
    <svg className="book-warp-defs" aria-hidden="true" width="0" height="0">
      <defs>
        <filter
          id="book-warp"
          x="0"
          y="0"
          width="1"
          height="1"
          primitiveUnits="objectBoundingBox"
          colorInterpolationFilters="sRGB"
        >
          <feImage
            href={maps.warp}
            x="0"
            y="0"
            width="1"
            height="1"
            preserveAspectRatio="none"
            result="map"
          />
          <feDisplacementMap
            in="SourceGraphic"
            in2="map"
            scale={maps.scale}
            xChannelSelector="R"
            yChannelSelector="G"
            result="bent"
          />
          <feImage
            href={maps.shade}
            x="0"
            y="0"
            width="1"
            height="1"
            preserveAspectRatio="none"
            result="light"
          />
          {/* Keep the cover's transparency, such as its rounded corners. */}
          <feComposite in="light" in2="bent" operator="in" result="lit" />
          <feMerge>
            <feMergeNode in="bent" />
            <feMergeNode in="lit" />
          </feMerge>
        </filter>
      </defs>
    </svg>
  );
}
