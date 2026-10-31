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
  invoke<number[]>("books_on_device", { mount });
export const sendBooks = (mount: string, ids: number[]) =>
  invoke<SendOutcome[]>("send_books", { mount, ids });
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
