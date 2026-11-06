import { useEffect, useState } from "react";
import { coverInfo, type Book, type CoverInfo } from "./api";

/** How long a busy library may hold up the panel before it opens without the
 * cover's size, which then follows when it arrives. */
const MAX_WAIT_MS = 150;

export interface PanelBook {
  book: Book;
  cover: CoverInfo | null;
}

/**
 * The book for the edit panel, along with its cover's size. Keeps showing the
 * previous book until the next one's size is known, a few milliseconds later,
 * so the panel changes in one paint instead of its buttons shifting after it.
 */
export function usePanelBook(book: Book | null): PanelBook | null {
  const [shown, setShown] = useState<PanelBook | null>(null);

  // Forgotten on deselecting, or reopening would flash the previous book.
  if (!book && shown) setShown(null);

  useEffect(() => {
    if (!book) return;
    let alive = true;
    const show = (cover: CoverInfo | null) => {
      if (alive) setShown({ book, cover });
    };
    const timer = setTimeout(
      () =>
        alive &&
        setShown((s) => (s?.book.id === book.id ? s : { book, cover: null })),
      MAX_WAIT_MS,
    );
    coverInfo(book.id)
      .then(show, () => show(null))
      .finally(() => clearTimeout(timer));
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [book]);

  if (!book || !shown) return null;
  // Edits to the open book show at once; only its cover's size can lag.
  return shown.book.id === book.id ? { ...shown, book } : shown;
}
