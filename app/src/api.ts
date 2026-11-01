// Typed wrappers over the Tauri commands in src-tauri/src/lib.rs. Shapes
// mirror the serde output of the Rust types.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
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
}

export interface Identifier {
  scheme: string | null;
  value: string;
}

export interface Book {
  id: number;
  title: string;
  authors: string[];
  series: string | null;
  series_index: string | null;
  tags: string[];
  language: string | null;
  publisher: string | null;
  description: string | null;
  identifiers: Identifier[];
  original_name: string;
  file_size: number;
  has_cover: boolean;
  /** The cover was chosen by the user rather than taken from the EPUB. */
  custom_cover: boolean;
  added_at: string;
  modified_at: string;
  /** Absolute path of the cover thumbnail. */
  cover: string | null;
}

export interface MetadataEdit {
  title: string;
  authors: string[];
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
      stage: "preparing" | "converting" | "copying";
      index: number;
      total: number;
      file: string;
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
    | { Ok: { device_path: string; content_id: string; bytes: number } }
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
}

/** A book as the device lists it. */
export interface DeviceBook {
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

export type RuleKind = "series" | "tag" | "author";

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
  erc_version: string;
  firmware: string;
  db_version: number | null;
}

export interface LogEntry {
  at: string;
  erc_version: string;
  op: string;
  content_ids?: string[];
  detail?: string;
  backup?: string;
  /** Present when the operation failed. */
  error?: string;
}

/** ERC's own records on the device, from `/.erc/`. */
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

export type DeviceImport =
  | { kind: "added"; book_id: number }
  | { kind: "linked"; book_id: number }
  | { kind: "skipped"; reason: string };

export interface DeviceImportOutcome {
  content_id: string;
  title: string;
  result: { Ok: DeviceImport } | { Err: string };
}

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
export const importBooks = (paths: string[]) =>
  write<ImportResult[]>("library_import", { paths }, ["library", "plans"]);
export const updateBook = (id: number, edit: MetadataEdit) =>
  write<Book>("library_update", { id, edit }, [
    "library",
    "plans",
    coverSearchTag(id),
  ]);
export const removeBooks = (ids: number[]) =>
  write<void>("library_remove", { ids }, [
    "library",
    "plans",
    ...ids.map(coverSearchTag),
  ]);
export const booksOnDevice = (mount: string) =>
  read<OnDevice[]>("books_on_device", { mount }, [deviceTag(mount), "library"]);
export const deviceBooks = (mount: string) =>
  read<DeviceBook[]>("device_books", { mount }, [deviceTag(mount)]);
export const removeFromDevice = (mount: string, contentIds: string[]) =>
  write<RemoveOutcome[]>("remove_from_device", { mount, contentIds }, [
    deviceTag(mount),
  ]);
export const updateBooks = (mount: string, ids: number[]) =>
  write<SendOutcome[]>("update_books", { mount, ids }, [deviceTag(mount)]);
export const tidyDevice = (mount: string) =>
  write<number>("tidy_device", { mount }, [deviceTag(mount)]);
export const deviceHistory = (mount: string) =>
  read<DeviceHistory>("device_history", { mount }, [deviceTag(mount)]);
export const sendBooks = (
  mount: string,
  ids: number[],
  collection: string | null = null,
) =>
  write<SendOutcome[]>("send_books", { mount, ids, collection }, [
    deviceTag(mount),
  ]);
/** Adds books from the device to the library; all of them when `contentIds` is null. */
export const importFromDevice = (mount: string, contentIds: string[] | null) =>
  // Reads the device only, but adds library books and device-copy links.
  write<DeviceImportOutcome[]>("import_from_device", { mount, contentIds }, [
    "library",
    "plans",
    deviceTag(mount),
  ]);
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
    const result = await invoke<{ series: number; grouped: number }>(
      "reconcile_device",
      { mount },
    );
    if (result.series || result.grouped) readCache.invalidate(deviceTag(mount));
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
export const ejectDevice = (mount: string) =>
  write<void>("eject_device", { mount }, [deviceTag(mount)]);

export const onSendProgress = (
  handler: (p: Progress) => void,
): Promise<UnlistenFn> =>
  listen<Progress>("send-progress", (e) => handler(e.payload));

export const onImportProgress = (
  handler: (p: ImportProgress) => void,
): Promise<UnlistenFn> =>
  listen<ImportProgress>("import-progress", (e) => handler(e.payload));

/** URL for a cover thumbnail; `version` busts the webview cache after edits. */
export function coverSrc(book: Book): string | null {
  if (!book.cover) return null;
  const url = convertFileSrc(book.cover);
  // data: URLs (the browser dev mock) can't take a query string.
  return url.startsWith("data:")
    ? url
    : `${url}?v=${encodeURIComponent(book.modified_at)}`;
}

/** Prefer the cover actually on the Kobo, falling back to a library copy. */
export function deviceCoverSrc(
  book: DeviceBook,
  library?: Book,
): string | null {
  return book.cover
    ? convertFileSrc(book.cover)
    : library
      ? coverSrc(library)
      : null;
}
