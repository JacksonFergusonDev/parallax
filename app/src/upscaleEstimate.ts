import type { UpscaleProgress } from "./api";

/** Tiles to average over: long enough to steady, short enough to follow the
 * slowdown as the Mac heats up. */
const WINDOW = 6;
/** How long to assume a tile takes until one has been timed. */
const FIRST_TILE_MS = 3000;

export interface UpscaleStatus {
  /** How far the bar should be once `tileMs` has passed, from 0 to 1. */
  target: number;
  tileMs: number;
  /** Null until a tile has been timed. */
  secondsLeft: number | null;
}

/**
 * Follows a run through a queue of covers. Every tile is the same size, so the
 * time left is the tiles left times how long recent ones took. The clock
 * starts at the first tile, which keeps the model's load time out of it.
 */
export class UpscaleEstimate {
  private earlier = 0;
  private id: number | null = null;
  private done = 0;
  private total = 0;
  private last: number | null = null;
  private gaps: number[] = [];

  constructor(private readonly planned: number) {}

  record(progress: UpscaleProgress, now: number): UpscaleStatus {
    if (progress.id !== this.id) {
      this.earlier += this.total;
      this.id = progress.id;
    }
    this.done = progress.done;
    this.total = progress.total;
    if (this.last !== null)
      this.gaps = [...this.gaps, now - this.last].slice(-WINDOW);
    this.last = now;

    const finished = this.earlier + this.done;
    const planned = Math.max(this.planned, this.earlier + this.total);
    const timed = this.gaps.length > 0;
    const tileMs = timed
      ? this.gaps.reduce((a, b) => a + b) / this.gaps.length
      : FIRST_TILE_MS;
    return {
      // Heads for the end of the tile now running, so the bar keeps moving
      // between events, yet never gets more than a tile ahead of the work.
      target: Math.min(1, (finished + 1) / planned),
      tileMs,
      secondsLeft: timed ? ((planned - finished) * tileMs) / 1000 : null,
    };
  }
}

export function timeLeft(seconds: number | null): string {
  if (seconds === null) return "Estimating time left…";
  if (seconds < 1) return "Finishing…";
  if (seconds < 55) return `About ${Math.ceil(seconds / 5) * 5}s left`;
  if (seconds < 90) return "About a minute left";
  return `About ${Math.round(seconds / 60)} min left`;
}
