// The reader's whole view of the app. The window's label picks the book in
// Rust, so nothing here can ask for another one.
import { invoke } from "@tauri-apps/api/core";
import type { BookLoader } from "foliate-js/epub.js";

export type Theme = "auto" | "light" | "sepia" | "dark";

export interface Settings {
  fontSize: number;
  theme: Theme;
}

export interface Position {
  href: string;
  cfi: string;
  fraction: number;
  quote: string | null;
}

export interface OpenedBook {
  title: string;
  entries: { name: string; size: number }[];
  position: Position | null;
  settings: Settings;
}

export const openedBook = () => invoke<OpenedBook>("reader_book");

export const savePosition = (position: Position) =>
  invoke<void>("reader_save_position", { position });

export const saveSettings = (settings: Settings) =>
  invoke<void>("reader_save_settings", { settings });

/** Feeds the engine one file at a time, as it asks for them. */
export function bookLoader(entries: OpenedBook["entries"]): BookLoader {
  const sizes = new Map(entries.map(({ name, size }) => [name, size]));
  const bytes = (name: string) => invoke<ArrayBuffer>("reader_entry", { name });
  const decoder = new TextDecoder();
  return {
    entries: entries.map(({ name }) => ({ filename: name })),
    getSize: (name) => sizes.get(name) ?? 0,
    loadText: async (name) =>
      sizes.has(name) ? decoder.decode(await bytes(name)) : null,
    loadBlob: async (name, type) =>
      sizes.has(name) ? new Blob([await bytes(name)], { type }) : null,
  };
}
