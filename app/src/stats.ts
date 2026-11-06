// Grouping and formatting for the Reading screen. Grouping happens here, not
// when the stats are read, so saved stats stay right as time passes.

import type { StatBook } from "./api";

export interface Bucket {
  /** "2024" or "2024-03". */
  key: string;
  label: string;
  count: number;
}

/** Books finished per year, from the first year with one to `now`'s year. */
export function finishedByYear(books: StatBook[], now: Date): Bucket[] {
  const counts = new Map<number, number>();
  for (const b of books) {
    if (!b.finished_at) continue;
    const year = new Date(b.finished_at).getFullYear();
    counts.set(year, (counts.get(year) ?? 0) + 1);
  }
  if (counts.size === 0) return [];
  const last = now.getFullYear();
  const first = Math.min(...counts.keys(), last);
  const out: Bucket[] = [];
  for (let y = first; y <= last; y++)
    out.push({ key: String(y), label: String(y), count: counts.get(y) ?? 0 });
  return out;
}

/** Books finished in each of the 12 months up to `now`'s month. */
export function finishedByMonth(books: StatBook[], now: Date): Bucket[] {
  const out: Bucket[] = [];
  for (let i = 11; i >= 0; i--) {
    const d = new Date(now.getFullYear(), now.getMonth() - i, 1);
    out.push({
      key: `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`,
      label: d.toLocaleString(undefined, { month: "short" }),
      count: 0,
    });
  }
  const index = new Map(out.map((b, i) => [b.key, i]));
  for (const b of books) {
    if (!b.finished_at) continue;
    const d = new Date(b.finished_at);
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
    const i = index.get(key);
    if (i !== undefined) out[i].count++;
  }
  return out;
}

/** "45 min", "3.2 h", "120 h". */
export function hours(secs: number | null | undefined): string {
  if (!secs) return "—";
  if (secs < 3600) return `${Math.max(1, Math.round(secs / 60))} min`;
  const h = secs / 3600;
  return `${h < 10 ? h.toFixed(1) : Math.round(h)} h`;
}

/** Clean axis ticks from 0 to at least `max`: 0, step, 2·step… */
export function ticks(max: number): number[] {
  if (max <= 0) return [0, 1];
  const rough = max / 3;
  const pow = 10 ** Math.floor(Math.log10(rough));
  const step = [1, 2, 5, 10].map((m) => m * pow).find((s) => s >= rough)!;
  const top = Math.ceil(max / step) * step;
  const out: number[] = [];
  for (let v = 0; v <= top + 1e-9; v += step) out.push(Math.round(v));
  return out;
}

/** Loose title match between Kobo stats and the library. */
export function titleKey(title: string): string {
  return title
    .toLowerCase()
    .replace(/[^\p{L}\p{N}]+/gu, " ")
    .trim();
}

/** One warm sentence summing up the reading, for the top of the screen. */
export function readingHeadline(
  stats: { total_secs: number; finished: StatBook[]; reading: StatBook[] },
  now: Date,
): string {
  const thisYear = stats.finished.filter(
    (b) =>
      b.finished_at &&
      new Date(b.finished_at).getFullYear() === now.getFullYear(),
  ).length;
  const time = spokenTime(stats.total_secs);
  const books = (n: number) => `${n} ${n === 1 ? "book" : "books"}`;
  if (thisYear > 0)
    return `${books(thisYear)} finished this year${time ? `, and ${time} spent reading` : ""}.`;
  if (stats.reading.length > 0)
    return `${books(stats.reading.length)} on the go${time ? `, ${time} read so far` : ""}.`;
  if (time) return `${time} spent reading on your Kobo.`;
  return "Your reading, at a glance.";
}

/** "16 hours", "1 hour", "40 minutes"; null for no time at all. */
export function spokenTime(secs: number | null | undefined): string | null {
  if (!secs) return null;
  if (secs < 3600) {
    const m = Math.max(1, Math.round(secs / 60));
    return `${m} ${m === 1 ? "minute" : "minutes"}`;
  }
  const h = Math.round(secs / 3600);
  return `${h} ${h === 1 ? "hour" : "hours"}`;
}

/** Below this much reading time a book was only opened, not started. */
export const STARTED_SECS = 30 * 60;

/** Books really being read: at least half an hour in. When the Kobo kept no
 * time for a book, a few percent of progress counts instead. */
export function startedBooks(reading: StatBook[]): StatBook[] {
  return reading.filter((b) =>
    b.time_secs != null ? b.time_secs >= STARTED_SECS : b.percent >= 5,
  );
}
