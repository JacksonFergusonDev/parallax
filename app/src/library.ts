// Pure helpers for the library view, kept out of components so they're testable.
import type { Book, ImportResult, MetadataEdit, Progress } from "./api";

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
