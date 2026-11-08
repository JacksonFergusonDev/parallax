// Highlights and notes: their colours on the page, the list of them, placing
// the little toolbar beside a selection, and copying them out.
import type { Highlight, HighlightColor } from "./backend";

export const COLORS: HighlightColor[] = ["yellow", "green", "blue", "pink"];

/** Drawn at the engine's 30% opacity, so they read on every theme. */
export const COLOR_VALUES: Record<HighlightColor, string> = {
  yellow: "#f2c200",
  green: "#3fb950",
  blue: "#3d8bfd",
  pink: "#ec5f9a",
};

export const COLOR_NAMES: Record<HighlightColor, string> = {
  yellow: "Yellow",
  green: "Green",
  blue: "Blue",
  pink: "Pink",
};

/** Long selections are kept whole but shown cut short in lists. */
export const MAX_SHOWN = 280;
/** What Rust accepts; longer selections are cut to this before saving. */
export const MAX_TEXT = 20_000;

export const shortened = (text: string, max = MAX_SHOWN) => {
  const flat = text.replace(/\s+/g, " ").trim();
  return flat.length > max ? `${flat.slice(0, max - 1).trimEnd()}…` : flat;
};

/** Reading order, then the order they were made in. */
export const inReadingOrder = (list: Highlight[]) =>
  [...list].sort((a, b) => a.fraction - b.fraction || a.id - b.id);

interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/**
 * Where to put a `size` toolbar for `target`: centred above it, or below
 * when there's no room above, and always inside the window.
 */
export function placeNear(
  target: Box,
  size: { width: number; height: number },
  viewport: { width: number; height: number },
  margin = 8,
): { left: number; top: number } {
  const centre = (target.left + target.right) / 2;
  const left = Math.min(
    Math.max(centre - size.width / 2, margin),
    Math.max(viewport.width - size.width - margin, margin),
  );
  const above = target.top - size.height - margin;
  const below = target.bottom + margin;
  const top =
    above >= margin
      ? above
      : Math.min(below, viewport.height - size.height - margin);
  return { left, top: Math.max(top, margin) };
}

/** A range's box in the reader window, from inside a page's frame. */
export function boxInWindow(range: Range): Box {
  const rect = range.getBoundingClientRect();
  const frame = range.startContainer.ownerDocument?.defaultView?.frameElement;
  const offset = frame?.getBoundingClientRect() ?? { left: 0, top: 0 };
  return {
    left: rect.left + offset.left,
    top: rect.top + offset.top,
    right: rect.right + offset.left,
    bottom: rect.bottom + offset.top,
  };
}

/** How far into its document a range starts, from 0 to 1, by text. */
export function offsetIn(range: Range): number {
  const doc = range.startContainer.ownerDocument;
  const body = doc?.body;
  if (!doc || !body) return 0;
  const total = body.textContent?.length ?? 0;
  if (!total) return 0;
  const before = doc.createRange();
  before.setStart(body, 0);
  try {
    before.setEnd(range.startContainer, range.startOffset);
  } catch {
    return 0;
  }
  return Math.min(before.toString().length / total, 1);
}

/** The list in the Notes panel. Text comes from the book, so it's text only. */
export function highlightList(
  list: Highlight[],
  chapterOf: (highlight: Highlight) => string,
  choose: (highlight: Highlight) => void,
): HTMLOListElement {
  const ol = document.createElement("ol");
  for (const highlight of inReadingOrder(list)) {
    const item = document.createElement("li");
    const button = document.createElement("button");
    button.dataset.color = highlight.color;
    const chapter = document.createElement("span");
    chapter.className = "chapter";
    chapter.textContent = chapterOf(highlight);
    const quote = document.createElement("span");
    quote.className = "quote";
    quote.textContent = shortened(highlight.text);
    button.append(chapter, quote);
    if (highlight.note) {
      const note = document.createElement("span");
      note.className = "note";
      note.textContent = highlight.note;
      button.append(note);
    }
    button.addEventListener("click", () => choose(highlight));
    item.append(button);
    ol.append(item);
  }
  return ol;
}

/** All of a book's highlights as Markdown, for pasting elsewhere. */
export function asMarkdown(
  title: string,
  list: Highlight[],
  chapterOf: (highlight: Highlight) => string,
): string {
  const lines = [`# ${title}`, ""];
  let chapter: string | null = null;
  for (const highlight of inReadingOrder(list)) {
    const label = chapterOf(highlight);
    if (label !== chapter) {
      chapter = label;
      if (label) lines.push(`## ${label}`, "");
    }
    const quote = highlight.text.replace(/\s+/g, " ").trim();
    lines.push(`> ${quote}`, "");
    if (highlight.note) lines.push(highlight.note.trim(), "");
  }
  return lines.join("\n");
}
