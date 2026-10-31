// Typed wrappers over the Tauri commands in src-tauri/src/lib.rs. Shapes
// mirror the serde output of the Rust types.
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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

export interface CoverCandidate {
  source: string;
  url: string;
  preview_url: string;
}

export const listDevices = () => invoke<DeviceInfo[]>("list_devices");
export const listBooks = () => invoke<Book[]>("library_list");
export const importBooks = (paths: string[]) =>
  invoke<ImportResult[]>("library_import", { paths });
export const updateBook = (id: number, edit: MetadataEdit) =>
  invoke<Book>("library_update", { id, edit });
export const removeBooks = (ids: number[]) =>
  invoke<void>("library_remove", { ids });
export const booksOnDevice = (mount: string) =>
  invoke<OnDevice[]>("books_on_device", { mount });
export const deviceBooks = (mount: string) =>
  invoke<DeviceBook[]>("device_books", { mount });
export const removeFromDevice = (mount: string, contentIds: string[]) =>
  invoke<RemoveOutcome[]>("remove_from_device", { mount, contentIds });
export const updateBooks = (mount: string, ids: number[]) =>
  invoke<SendOutcome[]>("update_books", { mount, ids });
export const tidyDevice = (mount: string) =>
  invoke<number>("tidy_device", { mount });
export const deviceHistory = (mount: string) =>
  invoke<DeviceHistory>("device_history", { mount });
export const sendBooks = (
  mount: string,
  ids: number[],
  collection: string | null = null,
) => invoke<SendOutcome[]>("send_books", { mount, ids, collection });
export const listCollections = (mount: string) =>
  invoke<Collection[]>("list_collections", { mount });
export const changeCollections = (mount: string, changes: CollectionChange[]) =>
  invoke<void>("change_collections", { mount, changes });
export const folderGroups = (mount: string) =>
  invoke<FolderGroup[]>("folder_groups", { mount });
export const reconcileDevice = (mount: string) =>
  invoke<{ series: number; grouped: number }>("reconcile_device", { mount });
export const rulesList = () => invoke<Rule[]>("rules_list");
export const rulesSave = (rule: Rule) => invoke<Rule>("rules_save", { rule });
export const rulesDelete = (id: number) => invoke<void>("rules_delete", { id });
export const rulesPreview = (mount: string) =>
  invoke<PlannedCollection[]>("rules_preview", { mount });
export const rulesApply = (mount: string) =>
  invoke<number>("rules_apply", { mount });
export const searchCovers = (id: number) =>
  invoke<CoverCandidate[]>("cover_search", { id });
export const setCoverFromUrl = (id: number, url: string) =>
  invoke<Book>("cover_set_url", { id, url });
export const setCoverFromFile = (id: number, path: string) =>
  invoke<Book>("cover_set_file", { id, path });
export const resetCover = (id: number) => invoke<Book>("cover_reset", { id });
export const ejectDevice = (mount: string) =>
  invoke<void>("eject_device", { mount });

export const onSendProgress = (
  handler: (p: Progress) => void,
): Promise<UnlistenFn> =>
  listen<Progress>("send-progress", (e) => handler(e.payload));

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
