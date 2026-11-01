// Pure helpers for the library view, kept out of components so they're testable.
import type { Book, ImportResult, MetadataEdit, Progress } from "./api";
import { collator, type Column, type SortValue } from "./sort";

export type SendStatus =
  | { stage: "queued" | "preparing" | "converting" | "copying" }
  | { stage: "sent" }
  | { stage: "failed"; error: string };

export function filterBooks(books: Book[], query: string): Book[] {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return books;
  return books.filter((b) => {
    const haystack = [b.title, ...b.authors, b.series ?? "", ...b.tags]
      .join(" ")
      .toLowerCase();
    return terms.every((t) => haystack.includes(t));
  });
}

/** Applies a progress event; `batch` is the id list passed to send. */
export function applyProgress(
  statuses: Record<number, SendStatus>,
  batch: number[],
  p: Progress,
): Record<number, SendStatus> {
  const id = batch[p.index - 1];
  if (id === undefined) return statuses;
  const next: SendStatus =
    p.stage === "done"
      ? { stage: "sent" }
      : p.stage === "failed"
        ? { stage: "failed", error: p.error }
        : { stage: p.stage };
  return { ...statuses, [id]: next };
}

export function summarizeImport(results: ImportResult[]): string {
  let added = 0;
  let existing = 0;
  let duplicates = 0;
  const failed: string[] = [];
  for (const r of results) {
    if ("Err" in r.outcome) failed.push(r.path.split(/[/\\]/).pop() ?? r.path);
    else if (r.outcome.Ok.kind === "already_in_library") existing++;
    else {
      added++;
      if (r.outcome.Ok.possible_duplicate_of !== null) duplicates++;
    }
  }
  const parts = [];
  if (added) parts.push(`Added ${added} ${added === 1 ? "book" : "books"}`);
  if (duplicates)
    parts.push(`${duplicates} may duplicate books you already have`);
  if (existing) parts.push(`${existing} already in your library`);
  if (failed.length) parts.push(`couldn't read ${failed.join(", ")}`);
  return parts.join(" · ") || "Nothing to import";
}

const splitList = (s: string) =>
  s
    .split(/[,;\n]/)
    .map((x) => x.trim())
    .filter(Boolean);

export interface EditForm {
  title: string;
  authors: string;
  series: string;
  seriesIndex: string;
  tags: string;
}

export const toForm = (b: Book): EditForm => ({
  title: b.title,
  authors: b.authors.join(", "),
  series: b.series ?? "",
  seriesIndex: b.series_index ?? "",
  tags: b.tags.join(", "),
});

export const fromForm = (f: EditForm): MetadataEdit => ({
  title: f.title.trim(),
  authors: splitList(f.authors),
  series: f.series.trim() || null,
  series_index:
    f.series.trim() && f.seriesIndex.trim() ? f.seriesIndex.trim() : null,
  tags: splitList(f.tags),
});

export const isBookFile = (path: string) => /\.(epub|kepub)$/i.test(path);

export type LibraryKey =
  "title" | "author" | "series" | "tags" | "progress" | "kobo" | "added";

/** "The Hobbit" sorts under H. */
export const titleKey = (t: string) => t.replace(/^(the|a|an)\s+/i, "");
/** "Frank Herbert" sorts under Herbert. */
export const surnameKey = (name: string) => {
  const parts = name.trim().split(/\s+/);
  return parts.length > 1
    ? `${parts.at(-1)} ${parts.slice(0, -1).join(" ")}`
    : name;
};
/** Series name then reading order; books without a number go last. */
export const seriesValue = (
  series: string | null,
  index: string | null,
): SortValue => {
  if (!series) return null;
  const n = parseFloat(index ?? "");
  return [series, Number.isFinite(n) ? n : Number.MAX_VALUE];
};

/** The library's default order: by title, ignoring leading articles. */
export const defaultOrder = (books: Book[]) =>
  [...books].sort(
    (a, b) =>
      collator.compare(titleKey(a.title), titleKey(b.title)) || a.id - b.id,
  );

/** On the Kobo: "stale" when edited since it was sent. */
export type DeviceState = "on" | "stale" | undefined;

export const libraryColumns = (
  deviceState: (b: Book) => DeviceState,
): Column<Book, LibraryKey>[] => [
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
    value: (b) => (b.authors[0] ? surnameKey(b.authors[0]) : null),
  },
  {
    key: "series",
    label: "Series",
    width: "minmax(0, 1.2fr)",
    value: (b) => seriesValue(b.series, b.series_index),
  },
  {
    key: "tags",
    label: "Tags",
    width: "minmax(0, 1fr)",
    value: (b) => (b.tags.length ? b.tags.join(", ") : null),
  },
  {
    key: "progress",
    label: "Progress",
    width: "80px",
    align: "end",
    // Descending: finished, then most read, then unread; never seen last.
    value: (b) =>
      !b.reading
        ? null
        : b.reading.status === "finished"
          ? 101
          : b.reading.status === "reading"
            ? b.reading.percent
            : -1,
  },
  {
    key: "kobo",
    label: "On Kobo",
    width: "92px",
    align: "end",
    // Descending puts books needing an update first, then the rest on the Kobo.
    value: (b) => ({ stale: 2, on: 1, none: 0 })[deviceState(b) ?? "none"],
  },
  {
    key: "added",
    label: "Added",
    width: "100px",
    align: "end",
    value: (b) => b.added_at,
  },
];

/** "Finished", "63%", "Unread", or "" when no Kobo has reported it. */
export function progressLabel(b: Book): string {
  switch (b.reading?.status) {
    case "finished":
      return "Finished";
    case "reading":
      return `${b.reading.percent}%`;
    case "unread":
      return "Unread";
    default:
      return "";
  }
}
