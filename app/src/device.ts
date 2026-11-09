// Pure helpers for the Kobo view.
import type { DeviceBook, DeviceImportOutcome } from "./api";
import { seriesValue, surnameKey, titleKey } from "./library";
import type { Column } from "./sort";

export function filterDeviceBooks(
  books: DeviceBook[],
  query: string,
): DeviceBook[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return books;
  return books.filter((b) => {
    const haystack = [b.title, b.author ?? "", b.series ?? "", ...b.collections]
      .join(" ")
      .toLowerCase();
    return terms.every((t) => haystack.includes(t));
  });
}

const books = (n: number) => `${n} ${n === 1 ? "book" : "books"}`;

/** One line describing an import from the device. */
export function summarizeDeviceImport(outcomes: DeviceImportOutcome[]): string {
  let added = 0;
  let linked = 0;
  let failed = 0;
  const skipped = new Map<string, number>();
  for (const o of outcomes) {
    if ("Err" in o.result) failed++;
    else if (o.result.Ok.kind === "added") added++;
    else if (o.result.Ok.kind === "linked") linked++;
    else {
      const reason = o.result.Ok.reason;
      skipped.set(reason, (skipped.get(reason) ?? 0) + 1);
    }
  }
  const parts = [];
  if (added) parts.push(`Added ${books(added)} to your library`);
  if (linked) parts.push(`matched ${books(linked)} you already had`);
  for (const [reason, n] of skipped)
    parts.push(`${books(n)} skipped: ${reason}`);
  if (failed) parts.push(`${books(failed)} couldn't be read`);
  return parts.join(" · ") || "Nothing to add";
}

export function statusLabel(b: DeviceBook): string {
  switch (b.read_status) {
    case "finished":
      return "Finished";
    case "reading":
      return `${b.percent_read}%`;
    default:
      return "Unread";
  }
}

export function summarize(books: DeviceBook[]): string {
  const reading = books.filter((b) => b.read_status === "reading").length;
  const finished = books.filter((b) => b.read_status === "finished").length;
  const pending = books.filter((b) => b.pending_import).length;
  const parts = [`${books.length} ${books.length === 1 ? "book" : "books"}`];
  if (reading) parts.push(`${reading} in progress`);
  if (finished) parts.push(`${finished} finished`);
  if (pending) parts.push(`${pending} waiting to import`);
  return parts.join(" · ");
}

/** "3 Jan 2025", or a relative day for recent dates. */
export function shortDate(iso: string | null, now = new Date()): string {
  if (!iso) return "—";
  const d = new Date(iso);
  const days = Math.floor((now.getTime() - d.getTime()) / 86_400_000);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  if (days < 7) return `${days} days ago`;
  return d.toLocaleDateString(undefined, {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

export type DeviceKey =
  "title" | "author" | "series" | "status" | "last_read" | "added" | "color";

export const deviceColumns: Column<DeviceBook, DeviceKey>[] = [
  {
    key: "title",
    label: "Title",
    width: "minmax(0, 2fr)",
    value: (b) => titleKey(b.title),
  },
  {
    key: "author",
    label: "Author",
    width: "minmax(0, 1.2fr)",
    value: (b) => (b.author ? surnameKey(b.author) : null),
  },
  {
    key: "series",
    label: "Series",
    width: "minmax(0, 1.2fr)",
    value: (b) => seriesValue(b.series, b.series_number),
  },
  {
    key: "status",
    label: "Status",
    width: "76px",
    align: "end",
    // Descending: finished, then most read, then unread.
    value: (b) =>
      b.read_status === "finished"
        ? 101
        : b.read_status === "reading"
          ? b.percent_read
          : -1,
  },
  {
    key: "last_read",
    label: "Last read",
    width: "100px",
    align: "end",
    value: (b) => b.last_read,
  },
  {
    key: "added",
    label: "On Kobo since",
    width: "100px",
    align: "end",
    value: (b) => b.added,
  },
];
