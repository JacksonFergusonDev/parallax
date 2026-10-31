// Pure helpers for the Kobo view.
import type { DeviceBook } from "./api";

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
