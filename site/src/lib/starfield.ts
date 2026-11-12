/**
 * The hero's moving background: three layers of stars drifting at
 * different speeds, so the near ones pass the far ones. That shift between
 * layers is parallax, the effect the app is named for. house-style's
 * runField decides when to draw (reduced motion, off screen, the pause button);
 * this only draws.
 */
import { runField } from 'house-style/js/field.js';

export type StarfieldVariant = 'landing' | 'diagonal';

const scenes = {
  landing: { seed: 20261110, slope: 0 },
  diagonal: { seed: 20261111, slope: 0.45 },
};

interface Star {
  x: number; // 0 to 1 across the canvas
  y: number; // 0 to 1 down the canvas
  r: number; // radius in CSS pixels
}

interface Layer {
  stars: Star[];
  speed: number; // canvas widths per second
  alpha: number;
}

// A small seeded generator, so every visitor sees the same sky.
function random(seed: number): () => number {
  let s = seed;
  return () => {
    s = (s * 1664525 + 1013904223) % 4294967296;
    return s / 4294967296;
  };
}

function makeLayers(variant: StarfieldVariant): Layer[] {
  const next = random(scenes[variant].seed);
  const layer = (count: number, size: number, speed: number, alpha: number): Layer => ({
    stars: Array.from({ length: count }, () => ({
      x: next(),
      y: next(),
      r: size * (0.6 + next() * 0.8),
    })),
    speed,
    alpha,
  });
  if (variant === 'diagonal') {
    return [layer(110, 0.65, 0.003, 0.4), layer(52, 1, 0.006, 0.6), layer(20, 1.4, 0.012, 0.85)];
  }
  return [layer(140, 0.7, 0.004, 0.45), layer(70, 1.1, 0.009, 0.7), layer(28, 1.6, 0.018, 1)];
}

function wrap(position: number): number {
  return position - Math.floor(position);
}

export function startStarfield(
  canvas: HTMLCanvasElement,
  button: HTMLButtonElement,
  variant: StarfieldVariant,
): void {
  const context = canvas.getContext('2d');
  if (!context) return; // The hero keeps its plain background.
  const ctx = context;
  const layers = makeLayers(variant);
  const { slope } = scenes[variant];
  let width = 1;
  let height = 1;
  let color = 'rgb(245, 197, 104)';
  let strength = 0.36;
  let blend: GlobalCompositeOperation = 'lighter';

  runField(canvas, button, {
    style(style) {
      color = `rgb(${style.color.join(', ')})`;
      strength = style.strength;
      blend = style.blend === 'add' ? 'lighter' : 'source-over';
    },
    resize(w, h) {
      const ratio = Math.min(window.devicePixelRatio || 1, 2);
      width = w;
      height = h;
      canvas.width = Math.round(w * ratio);
      canvas.height = Math.round(h * ratio);
      ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    },
    draw(time) {
      ctx.clearRect(0, 0, width, height);
      ctx.globalCompositeOperation = blend;
      ctx.fillStyle = color;
      for (const layer of layers) {
        ctx.globalAlpha = Math.min(1, layer.alpha * strength * 2.2);
        const shiftX = time * layer.speed;
        const shiftY = (shiftX * slope * width) / height;
        for (const star of layer.stars) {
          const x = wrap(star.x - shiftX) * width;
          const y = wrap(star.y - shiftY) * height;
          ctx.beginPath();
          ctx.arc(x, y, star.r, 0, Math.PI * 2);
          ctx.fill();
        }
      }
      ctx.globalAlpha = 1;
    },
  });
}
