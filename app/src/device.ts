// Pure helpers for the Kobo view.
import type { DeviceBook } from "./api";
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
  const parts = [`${books.length} ${books.length === 1 ? "book" : "books"}`];
  if (reading) parts.push(`${reading} in progress`);
  if (finished) parts.push(`${finished} finished`);
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
  "title" | "author" | "series" | "status" | "last_read" | "added";

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
