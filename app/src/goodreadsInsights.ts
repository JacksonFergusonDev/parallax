// Wording, grouping and table order for the Goodreads part of Reading. The
// numbers come from crates/core/src/goodreads_insights.rs; this only decides
// how they read.

import type { GoodreadsBook, GoodreadsImport, GoodreadsInsights } from "./api";
import type { Bucket } from "./stats";
import type { Bar } from "./components/charts";

export type InsightBook = GoodreadsInsights["books"][number];

const books = (n: number) =>
  `${n.toLocaleString()} ${n === 1 ? "book" : "books"}`;

export const dateFmt = (date: string | null | undefined) =>
  date
    ? new Date(
        date.length === 10 ? `${date}T00:00:00` : date,
      ).toLocaleDateString(undefined, {
        day: "numeric",
        month: "short",
        year: "numeric",
      })
    : "";

/** One warm sentence for the top of the screen. */
export function insightsHeadline(i: GoodreadsInsights): string {
  const pace = i.pace;
  if (pace && pace.this_year > 0) {
    const sofar = `${books(pace.this_year)} read so far this year`;
    if (pace.last_year_by_now > 0 && pace.last_year_by_now !== pace.this_year)
      return `${sofar}, against ${pace.last_year_by_now} by now last year.`;
    return `${sofar}.`;
  }
  if (i.totals.read > 0)
    return i.totals.to_read > 0
      ? `${books(i.totals.read)} read, and ${i.totals.to_read.toLocaleString()} more waiting.`
      : `${books(i.totals.read)} read.`;
  return "Your Goodreads shelves, at a glance.";
}

/** Where the books came from: Goodreads doesn't date its exports, so the
 * file's date stands in for it. */
export function importLine(
  imp: GoodreadsImport | null,
  imports: number,
): string {
  if (!imp) return "From your saved Goodreads books";
  const parts = [
    imp.file_modified
      ? `From your Goodreads export of ${dateFmt(imp.file_modified)}`
      : imp.latest_activity
        ? `From your Goodreads export, with activity up to ${dateFmt(imp.latest_activity)}`
        : "From your Goodreads export",
    `imported ${dateFmt(imp.imported_at)}`,
  ];
  if (imports > 1) parts.push(`${imports} imports so far`);
  return parts.join(" · ");
}

/** "3 weeks", "5 months", "2.5 years". */
export function spokenDays(days: number): string {
  if (days < 14) return `${days} ${days === 1 ? "day" : "days"}`;
  if (days < 60) return `${Math.round(days / 7)} weeks`;
  if (days < 365 * 1.5) {
    const m = Math.round(days / 30.4);
    return `${m} ${m === 1 ? "month" : "months"}`;
  }
  const y = Math.round((days / 365) * 2) / 2;
  return `${y % 1 ? y.toFixed(1) : y} years`;
}

export function statusLabel(shelf: GoodreadsBook["shelf"]): string {
  if (typeof shelf === "object") return shelf.other;
  return { read: "Read", to_read: "To read", currently_reading: "Reading" }[
    shelf
  ];
}

export function yearBuckets(
  years: GoodreadsInsights["years"],
  metric: "books" | "pages",
): Bucket[] {
  return years.map((y) => ({
    key: String(y.year),
    label: String(y.year),
    count: y[metric],
  }));
}

export function decadeBuckets(decades: GoodreadsInsights["decades"]): Bucket[] {
  return decades.map((d) => ({
    key: d.decade === null ? "before-1900" : String(d.decade),
    label: d.decade === null ? "Earlier" : `${d.decade}s`,
    tip: d.decade === null ? "before 1900" : `the ${d.decade}s`,
    count: d.read + d.to_read,
  }));
}

export function lengthBars(
  buckets: GoodreadsInsights["lengths"]["buckets"],
  shelf: "read" | "to_read",
): Bar[] {
  return buckets.map((b) => ({
    key: String(b.min),
    label:
      b.max === null
        ? `${b.min}+ pages`
        : b.min === 0
          ? `Under ${b.max}`
          : `${b.min}–${b.max - 1}`,
    value: b[shelf],
  }));
}

export function ratingBars(
  stars: GoodreadsInsights["ratings"]["stars"],
): Bar[] {
  return [5, 4, 3, 2, 1].map((n) => ({
    key: String(n),
    label: `${"★".repeat(n)}`,
    value: stars[n - 1],
  }));
}

export type StatusFilter = "all" | "read" | "to_read" | "currently_reading";
export type SortKey =
  "title" | "author" | "status" | "rating" | "date" | "pages" | "year";
export interface TableFilter {
  status: StatusFilter;
  /** Only books matching one in your library. */
  inLibrary: boolean;
}
export interface TableSort {
  key: SortKey;
  dir: "asc" | "desc";
}

/** The date a row shows: when read, or else when added. */
export const rowDate = (b: GoodreadsBook) => b.date_read ?? b.date_added;

const sortValue = (b: InsightBook, key: SortKey): string | number | null => {
  switch (key) {
    case "title":
      return b.title.toLowerCase();
    case "author":
      return (b.authors[0] ?? "").toLowerCase();
    case "status":
      return statusLabel(b.shelf);
    case "rating":
      return b.rating;
    case "date":
      return rowDate(b);
    case "pages":
      return b.pages;
    case "year":
      return b.original_year ?? b.year_published;
  }
};

/** Books on a status matching a search, in the chosen order. Books missing
 * the sorted value go last either way. */
export function tableRows(
  rows: InsightBook[],
  filter: TableFilter,
  query: string,
  sort: TableSort,
): InsightBook[] {
  const q = query.trim().toLowerCase();
  const { status, inLibrary } = filter;
  const matching = rows.filter(
    (b) =>
      (status === "all" || b.shelf === status) &&
      (!inLibrary || b.library.length > 0) &&
      (!q ||
        b.title.toLowerCase().includes(q) ||
        b.authors.some((a) => a.toLowerCase().includes(q)) ||
        (b.series?.name.toLowerCase().includes(q) ?? false)),
  );
  const dir = sort.dir === "asc" ? 1 : -1;
  return matching.sort((a, b) => {
    const x = sortValue(a, sort.key);
    const y = sortValue(b, sort.key);
    if (x === null || y === null) return x === y ? 0 : x === null ? 1 : -1;
    const c = x < y ? -1 : x > y ? 1 : 0;
    return c * dir || a.title.localeCompare(b.title);
  });
}

/** The first way a column sorts: names A to Z, numbers and dates biggest first. */
export const firstDir = (key: SortKey): TableSort["dir"] =>
  key === "title" || key === "author" || key === "status" ? "asc" : "desc";
