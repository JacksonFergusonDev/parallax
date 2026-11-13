// Typed wrappers over the Tauri commands in src-tauri/src/lib.rs. Shapes
// mirror the serde output of the Rust types.
import { Channel, convertFileSrc, invoke } from "@tauri-apps/api/core";
import type { Settings as ReaderSettings } from "./reader/backend";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { readCache } from "./readCache";

export interface DeviceInfo {
  kind: "kobo";
  model: string;
  firmware: string;
  serial: string;
  mount: string;
  capabilities: {
    native_collections: boolean;
    series: boolean;
    reading_progress: boolean;
    reading_time: boolean;
  };
  /** The screen comics are prepared for, in portrait pixels. */
  screen: Screen;
  /** Why the app won't change this device; null when it will. */
  read_only: string | null;
  /** A device of this model has been through the hardware checks. */
  tested: boolean;
}

export interface Screen {
  width: number;
  height: number;
  colour: boolean;
  /** A known model's size, measured from its covers, or assumed. */
  source: "model" | "covers" | "assumed";
}

export interface Identifier {
  scheme: string | null;
  value: string;
}

/** Someone other than an author, with a MARC relator role code. */
export interface Contributor {
  name: string;
  role: string;
}

export interface Book {
  id: number;
  /** From your saved Goodreads shelves; added by the app, not the library. */
  goodreads?: GoodreadsMatch;
  title: string;
  authors: string[];
  /** Translators, illustrators…; null when not read from the EPUB yet. */
  contributors?: Contributor[] | null;
  series: string | null;
  series_index: string | null;
  tags: string[];
  language: string | null;
  publisher: string | null;
  /** ISO 8601, to whatever precision the EPUB gives. */
  published: string | null;
  description: string | null;
  identifiers: Identifier[];
  /** Null when the EPUB couldn't be measured. */
  words: number | null;
  pages: number | null;
  /** Worked out from the words, rather than listed by the publisher. */
  pages_estimated: boolean;
  original_name: string;
  file_size: number;
  /** The stored file: EPUB, or CBZ for comics. */
  format: "epub" | "cbz";
  kind: "book" | "comic" | "manga";
  /** Which way pages turn; books are always left to right. */
  direction: "ltr" | "rtl";
  has_cover: boolean;
  /** The cover was chosen by the user rather than taken from the EPUB. */
  custom_cover: boolean;
  /** The custom cover came from the upscaler. */
  cover_upscaled: boolean;
  added_at: string;
  modified_at: string;
  /** Absolute path of the cover thumbnail. */
  cover: string | null;
  /** Progress last seen on a Kobo. */
  reading?: Reading | null;
  /** Highlights and notes copied from a Kobo. */
  annotations?: number;
}

/** A highlight, note or bookmark copied from a Kobo. */
export interface Annotation {
  id: string;
  device_serial: string;
  kind: "highlight" | "note" | "bookmark" | "markup";
  text: string | null;
  note: string | null;
  chapter: string | null;
  position: number;
  created: string | null;
  modified: string | null;
  /** False once deleted on the Kobo; the library keeps it. */
  on_device: boolean;
}

export interface Reading {
  device_serial: string;
  status: "unread" | "reading" | "finished";
  percent: number;
  last_read: string | null;
  time_spent_secs: number | null;
  seen_at: string;
}

export interface Mismatch {
  path: string;
  content_id: string;
  title: string;
}

/** What "Check this Kobo" found. */
export interface KoboHealth {
  missing_files: Mismatch[];
  not_imported: Mismatch[];
  duplicate_collections: { name: string; copies: number }[];
  orphaned_images: number;
  orphaned_image_bytes: number;
}

export interface MetadataEdit {
  title: string;
  authors: string[];
  /** null leaves the book's contributors as they are. */
  contributors: Contributor[] | null;
  series: string | null;
  series_index: string | null;
  tags: string[];
}

export type ImportOutcome =
  | {
      kind: "added";
      book: Omit<Book, "cover">;
      possible_duplicate_of: number | null;
    }
  | { kind: "already_in_library"; book: Omit<Book, "cover"> };

export interface ImportResult {
  path: string;
  outcome: { Ok: ImportOutcome } | { Err: string };
}

export type Progress =
  | {
      stage: "preparing" | "converting";
      index: number;
      total: number;
      file: string;
    }
  | {
      stage: "copying";
      index: number;
      total: number;
      file: string;
      bytes: number;
      total_bytes: number;
    }
  | {
      stage: "processing_comic";
      index: number;
      total: number;
      file: string;
      page: number;
      pages: number;
    }
  | {
      stage: "done";
      index: number;
      total: number;
      file: string;
      device_path: string;
    }
  | {
      stage: "failed";
      index: number;
      total: number;
      file: string;
      error: string;
    };

export interface SendOutcome {
  source: string;
  book_id: number | null;
  result:
    | {
        Ok: {
          device_path: string;
          content_id: string;
          bytes: number;
          /** Comic pages left out because they aren't readable images. */
          skipped_pages: string[];
          /** Only title, author and series changed; the file was left as it was. */
          details_only: boolean;
        };
      }
    | { Err: string };
}

/** A library book whose copy is on the connected device. */
export interface OnDevice {
  book_id: number;
  device_path: string;
  content_id: string;
  sent_at: string;
  /** Edited since it was sent. */
  stale: boolean;
  /** A comic on the Kobo as its original CBZ: updating changes only its details. */
  original_comic: boolean;
}

/** A book on the device, including sends that Nickel hasn't imported yet. */
export interface DeviceBook {
  pending_import?: boolean;
  /** Host-cached thumbnail read from the Kobo. */
  cover?: string | null;
  id: string;
  title: string;
  author: string | null;
  path: string | null;
  sideloaded: boolean;
  series: string | null;
  series_number: string | null;
  read_status: "unread" | "reading" | "finished";
  percent_read: number;
  last_read: string | null;
  reading_time_secs: number | null;
  added: string | null;
  file_size: number | null;
  collections: string[];
}

export interface RemoveOutcome {
  content_id: string;
  book_id?: number | null;
  result: { Ok: null } | { Err: string };
}

export interface Collection {
  name: string;
  book_count: number;
}

export type CollectionChange =
  | { kind: "create"; name: string }
  | { kind: "rename"; from: string; to: string }
  | { kind: "delete"; name: string }
  | { kind: "add"; name: string; content_ids: string[] }
  | { kind: "remove"; name: string; content_ids: string[] };

export interface FolderGroup {
  folder: string;
  content_ids: string[];
}

export type RuleKind = "series" | "genre" | "tag" | "author";

export interface Rule {
  /** 0 for a rule not saved yet. */
  id: number;
  kind: RuleKind;
  enabled: boolean;
  min_books: number;
  prefix: string;
}

export interface PlannedCollection {
  name: string;
  new: boolean;
  content_ids: string[];
  titles: string[];
}

export interface Stamp {
  at: string;
  parallax_version: string;
  firmware: string;
  db_version: number | null;
}

export interface LogEntry {
  at: string;
  parallax_version: string;
  op: string;
  content_ids?: string[];
  detail?: string;
  backup?: string;
  /** Present when the operation failed. */
  error?: string;
}

/** A database backup of a Kobo, taken just before a change. */
export interface BackupInfo {
  file: string;
  at: string;
  /** The change it was taken before. */
  entries: LogEntry[];
  /** Book files that change removed, still held on this computer. */
  removed_files: number;
  db_version: number | null;
  /** Why it can't be put back. */
  blocked: string | null;
}

/** A copy of the library's database, taken just before a big change. */
export interface Snapshot {
  file: string;
  at: string;
  op: string;
  detail: string | null;
  books: number;
  /** Why it can't be used. */
  blocked?: string;
}

/** Parallax's own records on the device, from `/.parallax/`. */
export interface DeviceHistory {
  device: {
    device_id: string;
    serial: string;
    first_write: Stamp;
    last_write: Stamp;
  } | null;
  books: { content_id: string; path: string }[];
  /** Newest first. */
  log: LogEntry[];
  problems: string[];
  changed: string[];
  missing: string[];
}

export type DuplicateReason =
  "same_file" | "same_isbn" | "same_title_and_author";

export interface LibraryDuplicateGroup {
  /** Identifies the group for "not duplicates". */
  key: string;
  reasons: DuplicateReason[];
  /** `remove`: suggested for removal. */
  members: { book_id: number; remove: boolean }[];
}

export interface DeviceDuplicateGroup {
  key: string;
  reasons: DuplicateReason[];
  members: {
    content_id: string;
    /** "KEPUB", "EPUB", "PDF"… or "Kobo store". */
    format: string;
    size: number | null;
    /** Sideloaded, so the app can remove it. */
    removable: boolean;
    remove: boolean;
  }[];
}

export interface ProposedSeries {
  name: string;
  index: string | null;
  /** "Kobo", "Wikidata"… */
  source: string;
  /** Selected by default. */
  confident: boolean;
}

export interface ProposedGenre {
  name: string;
  sources: string[];
  confident: boolean;
}

export interface CatalogRecord {
  catalog: "mangainfo" | "metron";
  url: string;
  issue_id: number | null;
}

export interface ComicDetails {
  authors: string[];
  contributors: Contributor[];
  description: string | null;
  publisher: string | null;
}

export interface Proposal {
  book_id: number;
  series: ProposedSeries | null;
  genres: ProposedGenre[];
  details?: ComicDetails & { confident: boolean };
  record?: CatalogRecord;
  issues?: { id: number; title: string }[];
  /** Sources that couldn't be asked. */
  problems: string[];
}

export interface Accepted {
  book_id: number;
  series: [string, string | null] | null;
  genres: string[];
  details?: ComicDetails;
  record?: CatalogRecord;
}

export type DeviceImport =
  | { kind: "added"; book_id: number }
  | { kind: "linked"; book_id: number }
  | { kind: "skipped"; reason: string };

export interface DeviceImportOutcome {
  content_id: string;
  title: string;
  result: { Ok: DeviceImport } | { Err: string };
}

export interface CalibrePreview {
  total: number;
  new: number;
  already: number;
  /** [formats, count] for books with no EPUB or KEPUB. */
  no_epub: [string, number][];
}

export type CalibreImport =
  | { kind: "added"; book_id: number }
  | { kind: "matched"; book_id: number }
  | { kind: "skipped"; reason: string };

export interface CalibreImportOutcome {
  calibre_id: number;
  title: string;
  result: { Ok: CalibreImport } | { Err: string };
}

export interface TagChange {
  from: string;
  to: string;
  book_ids: number[];
  /** A variant spelling; false when the tag only mentions the genre. */
  exact: boolean;
}

export interface CollectionTidy {
  from: string;
  to: string;
  books: number;
  merge: boolean;
}

export interface TidyPlan {
  tags: TagChange[];
  collections: CollectionTidy[];
}

export interface StatBook {
  content_id: string;
  title: string;
  author: string | null;
  percent: number;
  time_secs: number | null;
  left_secs: number | null;
  last_read: string | null;
  finished_at: string | null;
  times_finished: number;
  on_device: boolean;
}

export interface ReadingStats {
  device_model: string;
  total_secs: number;
  books_on_device: number;
  reading: StatBook[];
  finished: StatBook[];
  most_time: StatBook[];
}

export interface SavedStats {
  device_serial: string;
  taken_at: string;
  stats: ReadingStats;
}

export interface BookProgress {
  completed: number;
  total: number;
  title: string | null;
  /** Bytes saved for undo as a fraction of the book currently being removed. */
  file_fraction?: number;
}

export type ReportBookProgress = (progress: BookProgress) => void;

export interface ImportProgress {
  index: number;
  total: number;
  title: string;
}

export interface CoverCandidate {
  source: string;
  url: string;
  preview_url: string;
}

const deviceTag = (mount: string) => `device:${mount}`;
const coverSearchTag = (id: number) => `cover-search:${id}`;
let connectedDevices = new Map<string, string>();

function read<T>(
  command: string,
  args: Record<string, unknown>,
  tags: string[],
  maxAge?: number,
): Promise<T> {
  return readCache.get(
    [command, args],
    () => invoke<T>(command, args),
    tags,
    maxAge,
  );
}

async function write<T>(
  command: string,
  args: Record<string, unknown>,
  tags: string[],
): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } finally {
    // Failed batches can still have made partial changes. Invalidation also
    // removes pending reads, so an older response cannot refill the cache.
    readCache.invalidate(...tags);
  }
}

/** Only the command that owns the channel can report to this caller. */
function progressArgs<T>(report?: (event: T) => void) {
  return report ? { progress: new Channel(report) } : {};
}

/** Detection is always live; cached device data never crosses a USB session. */
export async function listDevices(): Promise<DeviceInfo[]> {
  let devices: DeviceInfo[];
  try {
    // Coalesce concurrent detections without reusing a completed result.
    devices = await readCache.get(
      ["list_devices"],
      () => invoke<DeviceInfo[]>("list_devices"),
      [],
      0,
    );
  } catch (e) {
    // Detection failure makes the USB session uncertain, just like disconnect.
    for (const mount of connectedDevices.keys())
      readCache.invalidate(deviceTag(mount));
    connectedDevices.clear();
    throw e;
  }
  const next = new Map(devices.map((d) => [d.mount, d.serial]));
  for (const mount of new Set([...connectedDevices.keys(), ...next.keys()])) {
    if (connectedDevices.get(mount) !== next.get(mount)) {
      readCache.invalidate(deviceTag(mount));
    }
  }
  connectedDevices = next;
  return devices;
}

export const listBooks = () => read<Book[]>("library_list", {}, ["library"]);
export const importBooks = (
  paths: string[],
  comicFolder?: boolean,
  report?: ReportBookProgress,
) =>
  write<ImportResult[]>(
    "library_import",
    { paths, ...(comicFolder ? { comicFolder } : {}), ...progressArgs(report) },
    ["library", "plans"],
  );
export const setComicKind = (
  id: number,
  kind: Book["kind"],
  direction: Book["direction"],
) =>
  write<Book>("library_set_comic_kind", { id, kind, direction }, [
    "library",
    "plans",
  ]);
export const comicSpreads = (id: number) =>
  read<number>("library_comic_spreads", { id }, ["library"]);
export const updateBook = (id: number, edit: MetadataEdit) =>
  write<Book>("library_update", { id, edit }, [
    "library",
    "plans",
    coverSearchTag(id),
  ]);
export const removeBooks = (ids: number[], report?: ReportBookProgress) =>
  write<void>("library_remove", { ids, ...progressArgs(report) }, [
    "library",
    "plans",
    ...ids.map(coverSearchTag),
  ]);
export interface GoodreadsShelfEntry {
  name: string;
  position: number | null;
}
export interface GoodreadsBook {
  goodreads_id: string;
  title: string;
  series: { name: string; number: string } | null;
  authors: string[];
  isbn: string | null;
  isbn13: string | null;
  publisher: string | null;
  binding: string | null;
  pages: number | null;
  year_published: number | null;
  original_year: number | null;
  /** 1 to 5, or null when unrated. */
  rating: number | null;
  /** Goodreads readers' average; only older exports have it. */
  average_rating?: number;
  shelf: "read" | "to_read" | "currently_reading" | { other: string };
  shelves: GoodreadsShelfEntry[];
  date_read: string | null;
  /** The date read was filled in when you marked it read here. */
  date_read_guessed?: boolean;
  date_added: string | null;
  read_count: number;
  review: string | null;
}
/** A book you changed here and the new export changed too, differently. */
export interface GoodreadsConflict {
  goodreads_id: string;
  title: string;
  authors: string[];
  changes: { field: string; label: string; mine: string; theirs: string }[];
}
/** A saved book the new export no longer has. */
export interface GoodreadsMissing {
  goodreads_id: string;
  title: string;
  authors: string[];
  status: string;
}
/** What importing an export would do, before anything is saved. */
export interface GoodreadsPlan {
  added: number;
  updated: number;
  unchanged: number;
  skipped: number;
  conflicts: GoodreadsConflict[];
  missing: GoodreadsMissing[];
  summary: GoodreadsExportSummary;
}
/** What the export holds, to show before importing. */
export interface GoodreadsExportSummary {
  books: number;
  read: number;
  to_read: number;
  currently_reading: number;
  /** Your own shelves, biggest first; `new` when not saved from an earlier export. */
  shelves: { name: string; books: number; new: boolean }[];
  /** Books in the export that match a book in your library. */
  in_library: number;
}
/** Decisions by Goodreads Book Id; anything left out gets the recommended choice. */
export interface GoodreadsResolutions {
  conflicts: Record<string, "mine" | "goodreads">;
  missing: Record<string, "keep" | "remove">;
}
export interface GoodreadsReport {
  added: number;
  updated: number;
  unchanged: number;
  skipped: number;
  kept: number;
  removed: number;
  kept_yours: number;
  took_goodreads: number;
}
export interface GoodreadsEdit {
  shelf?: GoodreadsBook["shelf"];
  /** 1 to 5; 0 clears it. */
  rating?: number;
  date_read?: string;
  clear_date_read?: boolean;
  add_shelves?: string[];
  remove_shelves?: string[];
}
export const goodreadsPlan = (path: string) =>
  invoke<GoodreadsPlan>("goodreads_plan", { path });
export const goodreadsImport = (
  path: string,
  resolutions: GoodreadsResolutions,
) =>
  write<GoodreadsReport>("goodreads_import", { path, resolutions }, [
    "goodreads",
  ]);
/** Where a library book sits on your Goodreads shelves. */
export interface GoodreadsMatch {
  goodreads_id: string;
  shelf: GoodreadsBook["shelf"];
  rating: number | null;
  date_read: string | null;
  /** Your own shelves it's on. */
  shelves: string[];
}
/** Library book ids on your saved Goodreads shelves. */
export const goodreadsMatches = () =>
  read<Record<string, GoodreadsMatch>>("goodreads_matches", {}, [
    "library",
    "goodreads",
  ]);
export const goodreadsEdit = (goodreadsId: string, edit: GoodreadsEdit) =>
  write<GoodreadsBook>("goodreads_edit", { goodreadsId, edit }, ["goodreads"]);
/** An export you imported, as recorded then. */
export interface GoodreadsImport {
  imported_at: string;
  file_name: string | null;
  /** When the file was last changed: usually when you downloaded it. */
  file_modified: string | null;
  /** The latest date added or read in the export. */
  latest_activity: string | null;
  books: number;
  added: number;
  updated: number;
}
export interface GoodreadsCount {
  name: string;
  books: number;
}
/** What your saved Goodreads shelves say about your reading. Books refer to
 * each other by Goodreads Book Id; `books` has every one. */
export interface GoodreadsInsights {
  import: GoodreadsImport | null;
  imports: number;
  totals: {
    books: number;
    read: number;
    to_read: number;
    reading: number;
    other: number;
    rated: number;
    reviewed: number;
    pages_read: number;
    read_undated: number;
    in_library: number;
  };
  /** Dated reads per year, first to this year, with empty years. */
  years: { year: number; books: number; pages: number }[];
  pace: {
    year: number;
    this_year: number;
    last_year_by_now: number;
    last_year: number;
  } | null;
  ratings: {
    /** Read books with each rating, one star first. */
    stars: [number, number, number, number, number];
    unrated: number;
    average: number | null;
    against_goodreads: {
      books: number;
      mine: number;
      goodreads: number;
    } | null;
  };
  to_read: {
    books: number;
    in_library: number;
    pages: number;
    median_age_days: number | null;
    oldest: string | null;
    years_to_clear: number | null;
    read_last_year: number;
  };
  wait: {
    books: number;
    median_days: number;
    longest: string;
    longest_days: number;
    logged_after: number;
  } | null;
  authors: {
    name: string;
    read: number;
    books: number;
    average: number | null;
  }[];
  series: {
    name: string;
    author: string | null;
    read: string[];
    unread: string[];
    missing: number[];
  }[];
  /** `decade` is null for anything before 1900. */
  decades: { decade: number | null; read: number; to_read: number }[];
  lengths: {
    buckets: {
      min: number;
      max: number | null;
      read: number;
      to_read: number;
    }[];
    average: number | null;
    longest: string | null;
    shortest: string | null;
  };
  formats: GoodreadsCount[];
  shelves: GoodreadsCount[];
  rereads: string[];
  books: (GoodreadsBook & { library: number[] })[];
}
export const goodreadsInsights = () =>
  read<GoodreadsInsights>("goodreads_insights", {}, ["library", "goodreads"]);
/** Insights already loaded, or undefined when they aren't yet. */
export const peekGoodreadsInsights = () =>
  readCache.peek<GoodreadsInsights>(["goodreads_insights", {}]);

/** Loads what Reading shows from the library in the background, so its
 * first opening draws at once. Only reads the library, never a device. */
export function prefetchReading() {
  void savedReadingStats().catch(() => undefined);
  void goodreadsInsights().catch(() => undefined);
}
export const librarySnapshots = () => invoke<Snapshot[]>("library_snapshots");
export const libraryRestore = (file: string) =>
  write<{
    safety_snapshot: string;
    restored: number;
    set_aside: number;
    lost: string[];
  }>("library_restore", { file }, ["library", "plans"]);
export const deviceBackups = (mount: string) =>
  invoke<BackupInfo[]>("device_backups", { mount });
export const deviceRestore = (mount: string, file: string) =>
  write<{ safety_backup: string; files_restored: number }>(
    "device_restore",
    { mount, file },
    [deviceTag(mount)],
  );
export const booksOnDevice = (mount: string, refresh = false) => {
  if (refresh) readCache.invalidate(deviceTag(mount));
  return read<OnDevice[]>("books_on_device", { mount }, [
    deviceTag(mount),
    "library",
  ]);
};

export const comicTransferEstimate = (mount: string, ids: number[]) =>
  invoke<{
    bytes: number;
    free: number;
    width: number;
    height: number;
    source: Screen["source"];
  } | null>("comic_transfer_estimate", { mount, ids });
export const deviceBooks = (mount: string) =>
  read<DeviceBook[]>("device_books", { mount }, [deviceTag(mount), "library"]);
export const removeFromDevice = (
  mount: string,
  contentIds: string[],
  report?: ReportBookProgress,
  expectedSerial?: string,
) =>
  write<RemoveOutcome[]>(
    "remove_from_device",
    { mount, contentIds, expectedSerial, ...progressArgs(report) },
    [deviceTag(mount)],
  );
export const updateBooks = (
  mount: string,
  ids: number[],
  report?: (p: Progress) => void,
) =>
  write<SendOutcome[]>(
    "update_books",
    { mount, ids, ...progressArgs(report) },
    [deviceTag(mount)],
  );
export const tidyDevice = (mount: string) =>
  write<number>("tidy_device", { mount }, [deviceTag(mount)]);
export const deviceHistory = (mount: string) =>
  read<DeviceHistory>("device_history", { mount }, [deviceTag(mount)]);
export const sendBooks = (
  mount: string,
  ids: number[],
  collection: string | null = null,
  report?: (p: Progress) => void,
) =>
  write<SendOutcome[]>(
    "send_books",
    { mount, ids, collection, ...progressArgs(report) },
    [deviceTag(mount)],
  );
/** Adds books from the device to the library; all of them when `contentIds` is null. */
export const importFromDevice = (
  mount: string,
  contentIds: string[] | null,
  report?: ReportBookProgress,
) =>
  // Reads the device only, but adds library books and device-copy links.
  write<DeviceImportOutcome[]>(
    "import_from_device",
    { mount, contentIds, ...progressArgs(report) },
    ["library", "plans", deviceTag(mount)],
  );
export const genresPlan = (mount: string | null) =>
  invoke<TidyPlan>("genres_plan", { mount });
export const genresApply = (
  mount: string | null,
  choice: { tags: [string, string][]; collections: [string, string][] },
) =>
  write<{ books: number; collections: number }>(
    "genres_apply",
    { mount, choice },
    ["library", "plans", ...(mount ? [deviceTag(mount)] : [])],
  );
/** Fresh from the Kobo at `mount` (and saved), else the last saved. */
const SAVED_STATS = ["reading_stats", { mount: null }] as const;
/** The reading stats last saved from a Kobo, cached like other reads. */
export const savedReadingStats = () =>
  read<SavedStats | null>(...SAVED_STATS, ["reading-stats"]);
/** Saved stats already loaded, or undefined when they aren't yet. */
export const peekSavedReadingStats = () =>
  readCache.peek<SavedStats | null>(SAVED_STATS);
/** Reads fresh stats from the Kobo at `mount`, which saves them, and makes
 * them the cached saved stats. Reads the device over USB, so call it when
 * someone looks at the stats rather than ahead of time. */
export async function refreshReadingStats(mount: string) {
  const stats = await invoke<SavedStats | null>("reading_stats", { mount });
  readCache.put(SAVED_STATS, stats, ["reading-stats"]);
  return stats;
}
export const calibreDefaultLibrary = () =>
  invoke<string | null>("calibre_default_library");
export const calibrePreview = (path: string) =>
  invoke<CalibrePreview>("calibre_preview", { path });
export const calibreImport = (path: string, report?: ReportBookProgress) =>
  // Reads the calibre library only.
  write<CalibreImportOutcome[]>(
    "calibre_import",
    { path, ...progressArgs(report) },
    ["library", "plans"],
  );
/** Looks up book metadata; `mount` adds a connected Kobo's clues. */
export const enhancePropose = (
  ids: number[],
  mount: string | null,
  metronIssue?: number,
) => invoke<Proposal[]>("enhance_propose", { ids, mount, metronIssue });
export const enhanceApply = (accepted: Accepted[]) =>
  write<Book[]>("enhance_apply", { accepted }, ["library", "plans"]);
export const openMetadataSource = (url: string) =>
  invoke<void>("open_metadata_source", { url });
/** Settings kept in the library; a missing value is the default. See
 * src-tauri/src/settings.rs for what each may hold. */
export type SettingKey =
  | "google_books_key"
  | "metron_token"
  | "review_on_add"
  | "theme"
  | "open_view"
  | "last_view"
  | "reader_windows"
  | "kobo_series"
  | "kobo_rules"
  | "eject_after_send"
  | "setup"
  | "kobo_notices"
  | "updates_auto"
  | "updates_download"
  | "updates_skipped"
  | "comics_kind"
  | "comic_spreads"
  | "comic_crop"
  | "comic_autocontrast"
  | "comic_upscale"
  | "comic_dither"
  | "comic_colour_separation"
  | "comic_colour_contrast"
  | "comic_midtone_darkening";

/** Every comic page setting; "Reset to recommended" clears them together.
 * Matches `COMIC_PAGE_SETTINGS` in crates/core/src/send.rs. */
export const COMIC_PAGE_SETTINGS = [
  "comic_spreads",
  "comic_crop",
  "comic_autocontrast",
  "comic_upscale",
  "comic_dither",
  "comic_colour_separation",
  "comic_colour_contrast",
  "comic_midtone_darkening",
] as const satisfies readonly SettingKey[];
/** Opens a Help page in the browser, as the Help menu does. */
export const openHelp = (page: "devices" | "issue" | "releases") =>
  invoke<void>("open_help", { page });
export const getSetting = (key: SettingKey) =>
  invoke<string | null>("setting_get", { key });
export const setSetting = (key: SettingKey, value: string | null) =>
  invoke<void>("setting_set", { key, value });
export const allSettings = () =>
  invoke<Partial<Record<SettingKey, string | null>>>("settings_all");
/** Puts preferences back to their defaults; keeps the Google Books key. */
export const resetSettings = () => invoke<void>("settings_reset");
export interface SettingChange {
  key: SettingKey;
  value: string | null;
}
/** Each setting change, from whichever window made it. */
export const onSettingChange = (
  handler: (change: SettingChange) => void,
): Promise<UnlistenFn> =>
  listen<SettingChange>("setting-changed", (e) => handler(e.payload));

/** A newer Parallax, found by the update check (src-tauri/src/updates.rs). */
export interface UpdateInfo {
  version: string;
  current: string;
  /** The release notes, in Markdown. */
  notes: string | null;
  /** Downloaded already, so installing only needs a restart. */
  ready: boolean;
}
export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}
/** The update a check already found, if the window missed its event. */
export const updateAvailable = () =>
  invoke<UpdateInfo | null>("update_available");
/** Installs the update, downloading it first unless it's ready, then
 * restarts Parallax. */
export const installUpdate = () => invoke<void>("update_install");
export const skipUpdate = (version: string) =>
  invoke<void>("update_skip", { version });
export const onUpdateAvailable = (
  handler: (update: UpdateInfo) => void,
): Promise<UnlistenFn> =>
  listen<UpdateInfo>("update-available", (e) => handler(e.payload));
export const onUpdateProgress = (
  handler: (progress: UpdateProgress) => void,
): Promise<UnlistenFn> =>
  listen<UpdateProgress>("update-progress", (e) => handler(e.payload));

export interface BackupPolicy {
  keep: "history" | "latest";
  /** The most the backup folder may take; null for no limit. */
  max_bytes: number | null;
}
export interface BackupSettings {
  policy: BackupPolicy;
  used_bytes: number;
}
export const backupSettings = () => invoke<BackupSettings>("backup_settings");
/** Saves the policy and applies it at once. */
export const setBackupSettings = (policy: BackupPolicy) =>
  invoke<BackupSettings>("backup_settings_set", { policy });
export const deleteAllBackups = () =>
  invoke<BackupSettings>("backup_delete_all");
export const showFolder = (folder: "library" | "backups" | "data") =>
  invoke<void>("show_folder", { folder });

export type { Settings as ReaderSettings } from "./reader/backend";
export const readerSettings = () =>
  invoke<ReaderSettings>("reader_settings_get");
export const setReaderSettings = (settings: ReaderSettings) =>
  invoke<void>("reader_settings_set", { settings });
/** Reader appearance changes, including those made in a reader. */
export const onReaderSettingsChange = (
  handler: (settings: ReaderSettings) => void,
): Promise<UnlistenFn> =>
  listen<ReaderSettings>("reader-settings-changed", (e) => handler(e.payload));
export const onEnhanceProgress = (
  handler: (p: ImportProgress) => void,
): Promise<UnlistenFn> =>
  listen<ImportProgress>("enhance-progress", (e) => handler(e.payload));
export type CheckKind =
  | "unreadable"
  | "drm"
  | "missing_files"
  | "broken_links"
  | "no_cover"
  | "small_cover"
  | "no_author"
  | "series_without_number"
  | "series_gap"
  | "unmanifested"
  | "clutter"
  | "embedded_fonts"
  | "damaged_pages";

export interface Finding {
  check: CheckKind;
  book_ids: number[];
  detail: string;
}

export type Issue =
  | "damaged"
  | "drm"
  | "no_genre"
  | "no_author"
  | "title_from_file"
  | "no_cover"
  | "small_cover";

export interface ReviewFinding {
  issue: Issue;
  book_id: number;
  detail: string;
}

/** What's worth raising about books just added. */
export interface Review {
  findings: ReviewFinding[];
  duplicates: LibraryDuplicateGroup[];
  /** Too many books to review without being asked. */
  bulk: boolean;
}

export const reviewBooks = (ids: number[]) =>
  invoke<Review>("library_review", { ids });

export const libraryCheck = () =>
  read<Finding[]>("library_check", {}, ["library"]);
export const onCheckProgress = (
  handler: (p: ImportProgress) => void,
): Promise<UnlistenFn> =>
  listen<ImportProgress>("check-progress", (e) => handler(e.payload));
export const annotationsList = (id: number) =>
  read<Annotation[]>("annotations_list", { id }, ["library"]);
/** Writes Markdown or JSON to `path`; returns how many annotations. */
export const annotationsExport = (
  id: number,
  format: "markdown" | "json",
  path: string,
) => invoke<number>("annotations_export", { id, format, path });
/** Saves a device report for adding support for new Kobos. Only reads. */
export const saveDeviceReport = (mount: string, path: string) =>
  invoke<void>("device_report", { mount, path });
export const koboHealth = (mount: string) =>
  read<KoboHealth>("kobo_health", { mount }, [deviceTag(mount)]);
/** Deletes leftover cover images; returns [files, bytes]. */
export const cleanImages = (mount: string) =>
  write<[number, number]>("clean_images", { mount }, [deviceTag(mount)]);
export const mergeCollections = (mount: string) =>
  write<number>("merge_collections", { mount }, [deviceTag(mount)]);
export const setReadingStatus = (
  mount: string,
  contentIds: string[],
  status: DeviceBook["read_status"],
  resetPosition: boolean,
  resetTime: boolean,
) =>
  write<number>(
    "set_reading_status",
    { mount, contentIds, status, resetPosition, resetTime },
    [deviceTag(mount), "library"],
  );
export const libraryDuplicates = () =>
  read<LibraryDuplicateGroup[]>("library_duplicates", {}, ["library"]);
export const deviceDuplicates = (mount: string) =>
  read<DeviceDuplicateGroup[]>("device_duplicates", { mount }, [
    deviceTag(mount),
  ]);
/** Hides a group from review; `mount` for groups on a device. */
export const dismissDuplicates = (key: string, mount: string | null) =>
  write<void>("dismiss_duplicates", { key, mount }, [
    "library",
    ...(mount ? [deviceTag(mount)] : []),
  ]);
export const listCollections = (mount: string) =>
  read<Collection[]>("list_collections", { mount }, [deviceTag(mount)]);
export const changeCollections = (mount: string, changes: CollectionChange[]) =>
  write<void>("change_collections", { mount, changes }, [deviceTag(mount)]);
export const folderGroups = (mount: string) =>
  read<FolderGroup[]>("folder_groups", { mount }, [deviceTag(mount)]);
export async function reconcileDevice(mount: string) {
  try {
    const result = await invoke<{
      series: number;
      grouped: number;
      reading: number;
      annotations: number;
    }>("reconcile_device", { mount });
    if (result.series || result.grouped) readCache.invalidate(deviceTag(mount));
    if (result.reading || result.annotations) readCache.invalidate("library");
    return result;
  } catch (e) {
    readCache.invalidate(deviceTag(mount));
    throw e;
  }
}
export const rulesList = () => read<Rule[]>("rules_list", {}, ["rules"]);
export const rulesSave = (rule: Rule) =>
  write<Rule>("rules_save", { rule }, ["rules", "plans"]);
export const rulesDelete = (id: number) =>
  write<void>("rules_delete", { id }, ["rules", "plans"]);
export const rulesPreview = (mount: string) =>
  read<PlannedCollection[]>("rules_preview", { mount }, [
    deviceTag(mount),
    "plans",
  ]);
export const rulesApply = (mount: string) =>
  write<number>("rules_apply", { mount }, [deviceTag(mount)]);
export const searchCovers = (id: number) =>
  read<CoverCandidate[]>(
    "cover_search",
    { id },
    [coverSearchTag(id)],
    10 * 60_000,
  );
export const setCoverFromUrl = (id: number, url: string) =>
  write<Book>("cover_set_url", { id, url }, ["library"]);
export const setCoverFromFile = (id: number, path: string) =>
  write<Book>("cover_set_file", { id, path }, ["library"]);
export const resetCover = (id: number) =>
  write<Book>("cover_reset", { id }, ["library"]);

/** An upscaled cover waiting to be kept or discarded. */
export interface UpscalePreview {
  id: number;
  run: number;
  /** Absolute paths of the images to compare. */
  before: string;
  after: string;
  before_size: [number, number];
  after_size: [number, number];
}

export type CoverQuality = "low" | "decent" | "excellent";

export interface CoverInfo {
  width: number;
  height: number;
  quality: CoverQuality;
}

export interface UpscaleProgress {
  id: number;
  done: number;
  total: number;
}

/** Opens the book in its own reader window, or brings that window forward. */
export const openReader = (id: number) => invoke<void>("reader_open", { id });

/** The size and rating of the cover sent to devices; null if there is none. */
export const coverInfo = (id: number) =>
  invoke<CoverInfo | null>("cover_info", { id });
/** Whether this build has the upscaling model. */
export const upscaleAvailable = () =>
  read<boolean>("cover_upscale_available", {}, []);
/** Tiles the upscaler will run for these books, for estimating time left. */
export const upscaleTiles = (ids: number[]) =>
  invoke<number>("cover_upscale_tiles", { ids });
/** Upscales without touching the book; null if already full size. */
export const previewUpscale = (id: number) =>
  invoke<UpscalePreview | null>("cover_upscale_preview", { id });
export const applyUpscale = (id: number, run: number) =>
  write<Book>("cover_upscale_apply", { id, run }, ["library"]);
/** Stops the running job and discards undecided previews. */
export const cancelUpscale = () => invoke<void>("cover_upscale_cancel");
export const onUpscaleProgress = (
  handler: (p: UpscaleProgress) => void,
): Promise<UnlistenFn> =>
  listen<UpscaleProgress>("upscale-progress", (e) => handler(e.payload));

/** Dominant colour of each cover file; null where one can't be read. */
export const coverColors = (paths: string[]) =>
  invoke<([number, number, number] | null)[]>("cover_colors", { paths });
export const ejectDevice = (mount: string) =>
  write<void>("eject_device", { mount }, [deviceTag(mount)]);

/** URL for a cover thumbnail; `version` busts the webview cache after edits. */
export function coverSrc(book: Book): string | null {
  if (!book.cover) return null;
  const url = convertFileSrc(book.cover);
  // data: URLs (the browser dev mock) can't take a query string.
  return url.startsWith("data:")
    ? url
    : `${url}?v=${encodeURIComponent(book.modified_at)}`;
}

/** Use original library artwork for linked books. Covers are sent sized
 * and greyed for greyscale Kobos, so Nickel's thumbnail can be greyscale on
 * an otherwise colour cover. */
export function deviceCoverPath(
  book: DeviceBook,
  library?: Book,
): string | null {
  return library?.cover ?? book.cover ?? null;
}

export function deviceCoverSrc(
  book: DeviceBook,
  library?: Book,
): string | null {
  const path = deviceCoverPath(book, library);
  if (!path) return null;
  return path === library?.cover ? coverSrc(library) : convertFileSrc(path);
}
