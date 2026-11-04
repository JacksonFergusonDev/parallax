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
