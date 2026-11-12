// Pure helpers for the library view, kept out of components so they're testable.
import { splitGenres } from "./genres";
import type {
  Book,
  Contributor,
  ImportResult,
  MetadataEdit,
  Progress,
  SendOutcome,
} from "./api";
import { collator, type Column, type SortValue } from "./sort";

export type SendStatus =
  | { stage: "queued" | "preparing" | "converting" }
  | { stage: "copying"; bytes: number; totalBytes: number }
  | { stage: "processing_comic"; page: number; pages: number }
  | { stage: "sent" }
  | { stage: "failed"; error: string };

const STATUS_LABEL: Record<
  Exclude<SendStatus["stage"], "processing_comic" | "copying">,
  string
> = {
  queued: "Waiting…",
  preparing: "Preparing…",
  converting: "Converting…",
  sent: "Sent",
  failed: "Failed",
};

export function sendStatusLabel(status: SendStatus): string {
  if (status.stage === "copying") {
    const { bytes, totalBytes } = status;
    if (totalBytes <= 0) return "Transferring…";
    if (bytes >= totalBytes) return "Finishing…";
    return `Transferring ${Math.floor((bytes / totalBytes) * 100)}%`;
  }
  return status.stage === "processing_comic"
    ? status.page === 0
      ? `Processing ${status.pages} pages…`
      : `Page ${status.page} of ${status.pages}`
    : STATUS_LABEL[status.stage];
}

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
        : p.stage === "processing_comic"
          ? { stage: p.stage, page: p.page, pages: p.pages }
          : p.stage === "copying"
            ? { stage: p.stage, bytes: p.bytes, totalBytes: p.total_bytes }
            : { stage: p.stage };
  return { ...statuses, [id]: next };
}

export function summarizeImport(results: ImportResult[]): string {
  let added = 0;
  let existing = 0;
  let duplicates = 0;
  const failed: string[] = [];
  for (const r of results) {
    if ("Err" in r.outcome) {
      const name = r.path.split(/[/\\]/).pop() ?? r.path;
      failed.push(
        BOOK_ARCHIVE.test(name) ? `${name} (${r.outcome.Err})` : name,
      );
    } else if (r.outcome.Ok.kind === "already_in_library") existing++;
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
  /** Tags that are standard genres, edited in their own field. */
  genres: string[];
  /** The other tags, comma-separated. */
  tags: string;
  contributors: Contributor[];
  /** The book's contributors were read; otherwise an empty list means "unchanged". */
  contributorsKnown: boolean;
}

/** MARC relator codes offered in the editor, most common first. */
export const ROLES: [string, string][] = [
  ["trl", "Translator"],
  ["ill", "Illustrator"],
  ["edt", "Editor"],
  ["art", "Artist"],
  ["clr", "Colourist"],
  ["nrt", "Narrator"],
  ["aui", "Introduction by"],
  ["aft", "Afterword by"],
  ["cov", "Cover designer"],
  ["pht", "Photographer"],
  ["ann", "Annotator"],
  ["ctb", "Contributor"],
];

export const toForm = (b: Book): EditForm => ({
  title: b.title,
  authors: b.authors.join(", "),
  series: b.series ?? "",
  seriesIndex: b.series_index ?? "",
  genres: splitGenres(b.tags).genres,
  tags: splitGenres(b.tags).others.join(", "),
  contributors: b.contributors ?? [],
  contributorsKnown: b.contributors != null,
});

export const fromForm = (f: EditForm): MetadataEdit => ({
  title: f.title.trim(),
  authors: splitList(f.authors),
  series: f.series.trim() || null,
  series_index:
    f.series.trim() && f.seriesIndex.trim() ? f.seriesIndex.trim() : null,
  // Genres are tags too: EPUB, calibre and the Kobo have no genre field.
  tags: [...f.genres, ...splitList(f.tags)],
  contributors:
    f.contributorsKnown || f.contributors.length
      ? f.contributors
          .map((c) => ({
            name: c.name.trim(),
            role: c.role.trim().toLowerCase() || "ctb",
          }))
          .filter((c) => c.name)
      : null,
});

const ARCHIVE_EXTENSIONS = ["zip", "rar", "7z"];
const BOOK_ARCHIVE = new RegExp(`\\.(${ARCHIVE_EXTENSIONS.join("|")})$`, "i");

/** Files accepted by Add Books, including archives containing books. */
export const BOOK_EXTENSIONS = [
  "epub",
  "kepub",
  "azw3",
  "azw",
  "mobi",
  "prc",
  "kfx",
  "cbz",
  "cbr",
  "cb7",
  ...ARCHIVE_EXTENSIONS,
];

const BOOK_FILE = new RegExp(`\\.(${BOOK_EXTENSIONS.join("|")})$`, "i");

export const isBookFile = (path: string) => BOOK_FILE.test(path);

export type LibraryKey =
  | "title"
  | "author"
  | "series"
  | "tags"
  | "pages"
  | "progress"
  | "kobo"
  | "added"
  | "color";

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

/** Group in current shelf order, with numbered volumes in reading order. */
export function seriesGroups(books: Book[]) {
  const groups = new Map<string | null, Book[]>();
  for (const book of books) {
    const name = book.series || null;
    const group = groups.get(name);
    if (group) group.push(book);
    else groups.set(name, [book]);
  }
  const volume = (book: Book) => {
    const n = parseFloat(book.series_index ?? "");
    return Number.isFinite(n) ? n : Number.MAX_VALUE;
  };
  return [...groups].map(([name, members]) => ({
    name,
    members: name ? members.sort((a, b) => volume(a) - volume(b)) : members,
  }));
}

/** Whether a copy exists and needs updating, independent of its import status. */
export type DeviceCopyState = "on" | "stale" | undefined;
/** The status shown next to a book for the connected Kobo. */
export type DeviceState =
  DeviceCopyState | "pending_import" | "updated" | "removed";

export const deviceStateLabel = {
  on: "On Kobo",
  stale: "Update available",
  pending_import: "Eject to import",
  updated: "Eject to see update",
  removed: "Removed from Kobo",
};

export const libraryColumns = (
  deviceState: (b: Book) => DeviceCopyState,
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
    key: "pages",
    label: "Pages",
    width: "64px",
    align: "end",
    value: (b) => b.pages,
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

/** "412", or "About 410" when worked out from the book's words. */
export function pagesLabel(b: Book): string | null {
  if (b.pages === null) return null;
  const count = b.pages.toLocaleString();
  return b.pages_estimated ? `About ${count}` : count;
}

/**
 * "1997" or "February 1997". Days are left out: EPUB dates are often shifted
 * by a time zone, or a placeholder for a book only known by its month.
 */
export function publishedLabel(date: string): string {
  const [, year, month] = /^(\d{4})(?:-(\d{2}))?/.exec(date) ?? [];
  if (!year) return date;
  if (!month) return year;
  return new Date(Date.UTC(+year, +month - 1)).toLocaleDateString(undefined, {
    month: "long",
    year: "numeric",
    timeZone: "UTC",
  });
}

/** "English" for "en"; codes the browser doesn't know stay as they are. */
export function languageLabel(code: string): string {
  try {
    return (
      new Intl.DisplayNames(undefined, { type: "language" }).of(code) ?? code
    );
  } catch {
    return code;
  }
}

/** The book's ISBN, given as an identifier scheme or a prefix on the value. */
export function isbnOf(b: Book): string | null {
  for (const { scheme, value } of b.identifiers) {
    if (scheme?.toLowerCase() === "isbn") return value;
    const prefixed = !scheme && /^isbn:\s*(.+)/i.exec(value);
    if (prefixed) return prefixed[1];
  }
  return null;
}

/** A description as plain text; EPUBs and calibre often store it as HTML. */
export function plainDescription(html: string): string {
  const spaced = html.replace(/<br\s*\/?>|<\/(p|div|li|h\d)>/gi, "$&\n");
  const doc = new DOMParser().parseFromString(spaced, "text/html");
  return (doc.body.textContent ?? "").replace(/\n\s*\n+/g, "\n").trim();
}

/** Finished or reading, from a Kobo or else your Goodreads shelves. */
export function readState(b: Book): "unread" | "reading" | "finished" {
  if (b.reading?.status === "finished" || b.reading?.status === "reading")
    return b.reading.status;
  if (b.goodreads?.shelf === "read") return "finished";
  if (b.goodreads?.shelf === "currently_reading") return "reading";
  return "unread";
}

/** "Finished", "63%", "Unread", "Read" or "Reading" (from Goodreads), or "". */
export function progressLabel(b: Book): string {
  switch (b.reading?.status) {
    case "finished":
      return "Finished";
    case "reading":
      return `${b.reading.percent}%`;
    default:
      break;
  }
  switch (b.goodreads?.shelf) {
    case "read":
      return "Read";
    case "currently_reading":
      return "Reading";
    default:
      return b.reading?.status === "unread" ? "Unread" : "";
  }
}

/** Names the damaged comic pages a transfer left out, if any. */
export function skippedPagesNote(outcomes: SendOutcome[]): string | null {
  const books = outcomes.flatMap(({ source, result }) =>
    "Ok" in result && result.Ok.skipped_pages.length > 0
      ? [`${source} (${result.Ok.skipped_pages.join(", ")})`]
      : [],
  );
  return books.length
    ? `Damaged pages were left out: ${books.join("; ")}. Check library lists them.`
    : null;
}
