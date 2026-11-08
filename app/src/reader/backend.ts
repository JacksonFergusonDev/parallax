// The reader's whole view of the app. The window's label picks the book in
// Rust, so nothing here can ask for another one.
import { invoke } from "@tauri-apps/api/core";
import type { BookLoader } from "foliate-js/epub.js";

export type Theme = "auto" | "light" | "sepia" | "dark";

export type Flow = "paginated" | "scrolled";
export type Measure = "narrow" | "medium" | "wide";
export type Spread = "auto" | "single";
export type Spacing = "book" | "relaxed" | "loose";

export interface Settings {
  fontSize: number;
  theme: Theme;
  flow: Flow;
  /** How long a line may get. */
  width: Measure;
  margins: Measure;
  spread: Spread;
  spacing: Spacing;
}

export const DEFAULT_SETTINGS: Settings = {
  fontSize: 100,
  theme: "auto",
  flow: "paginated",
  width: "medium",
  margins: "medium",
  spread: "auto",
  spacing: "book",
};

export type HighlightColor = "yellow" | "green" | "blue" | "pink";

/** A highlight as the reader makes it. */
export interface HighlightDraft {
  cfi: string;
  href: string;
  fraction: number;
  text: string;
  note: string | null;
  color: HighlightColor;
}

export interface Highlight extends HighlightDraft {
  id: number;
  createdAt: string;
  updatedAt: string;
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

export const openedBook = async (): Promise<OpenedBook> => {
  const book = await invoke<OpenedBook>("reader_book");
  return { ...book, settings: { ...DEFAULT_SETTINGS, ...book.settings } };
};

export const savePosition = (position: Position) =>
  invoke<void>("reader_save_position", { position });

export const saveSettings = (settings: Settings) =>
  invoke<void>("reader_save_settings", { settings });

interface StoredHighlight extends Omit<HighlightDraft, "color"> {
  id: number;
  color: string;
  created_at: string;
  updated_at: string;
}

const COLORS: HighlightColor[] = ["yellow", "green", "blue", "pink"];

const fromStored = ({
  created_at,
  updated_at,
  color,
  ...rest
}: StoredHighlight): Highlight => ({
  ...rest,
  color: COLORS.includes(color as HighlightColor)
    ? (color as HighlightColor)
    : "yellow",
  createdAt: created_at,
  updatedAt: updated_at,
});

export const highlights = async () =>
  (await invoke<StoredHighlight[]>("reader_highlights")).map(fromStored);

/** Adds `highlight`, or with `id` changes that one. */
export const saveHighlight = async (
  highlight: HighlightDraft,
  id: number | null = null,
) =>
  fromStored(
    await invoke<StoredHighlight>("reader_save_highlight", { id, highlight }),
  );

export const deleteHighlight = (id: number) =>
  invoke<void>("reader_delete_highlight", { id });

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
