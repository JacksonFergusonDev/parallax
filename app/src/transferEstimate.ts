import type { Progress } from "./api";

export interface BatchTransferProgress {
  completed: number;
  total: number;
  value: number;
  secondsLeft: number | null;
  activeComic: Extract<Progress, { stage: "processing_comic" }> | null;
}

// Comic preparation and USB copying report different units. Reserve part of
// each comic's share for copying and leave the last bit for its outcome.
const COMIC_PREPARATION_SHARE = 0.8;
const COPY_LIMIT = 0.99;

/** Estimates the time per volume from work observed in this sequential batch. */
export class TransferEstimate {
  private startedAt: number | null = null;
  private comic = false;
  private successful = 0;
  private successfulMs = 0;

  constructor(private readonly total: number) {}

  initial(): BatchTransferProgress {
    return {
      completed: 0,
      total: this.total,
      value: 0,
      secondsLeft: null,
      activeComic: null,
    };
  }

  record(progress: Progress, now: number): BatchTransferProgress {
    this.startedAt ??= now;
    const elapsed = now - this.startedAt;
    const settled = progress.stage === "done" || progress.stage === "failed";
    const completed = settled ? progress.index : progress.index - 1;
    let fraction = 0;

    if (progress.stage === "processing_comic") {
      this.comic = true;
      fraction = (COMIC_PREPARATION_SHARE * progress.page) / progress.pages;
    } else if (progress.stage === "copying") {
      const base = this.comic ? COMIC_PREPARATION_SHARE : 0;
      const copied = progress.total_bytes
        ? progress.bytes / progress.total_bytes
        : 0;
      fraction = base + (COPY_LIMIT - base) * copied;
    }

    if (progress.stage === "done" && elapsed > 0) {
      this.successful++;
      this.successfulMs += elapsed;
    }
    if (settled) {
      this.startedAt = null;
      this.comic = false;
    }

    const value = completed + fraction;
    // Wait for a meaningful sample and exclude failed volumes from the rate.
    const timedFraction = fraction >= 0.1 && elapsed >= 500 ? fraction : 0;
    const measured = this.successful + timedFraction;
    const measuredMs = this.successfulMs + (timedFraction ? elapsed : 0);
    const remaining = this.total - value;
    return {
      completed,
      total: this.total,
      value,
      activeComic: progress.stage === "processing_comic" ? progress : null,
      secondsLeft:
        remaining === 0
          ? 0
          : measured
            ? Math.max(1, (measuredMs * remaining) / measured / 1000)
            : null,
    };
  }
}
