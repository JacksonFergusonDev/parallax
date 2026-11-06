// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { usePanelBook, type PanelBook } from "./panelBook";
import type { Book, CoverInfo } from "./api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const book = (id: number, title = `Book ${id}`) => ({ id, title }) as Book;
const size = (height: number): CoverInfo => ({
  width: 100,
  height,
  quality: "low",
});

describe("usePanelBook", () => {
  let root: Root;
  let seen: PanelBook | null;
  let pending: Map<number, (info: CoverInfo | null) => void>;

  const Probe = ({ book }: { book: Book | null }) => {
    seen = usePanelBook(book);
    return null;
  };
  const select = (b: Book | null) =>
    act(async () => root.render(<Probe book={b} />));
  const answer = (id: number, info: CoverInfo | null) =>
    act(async () => pending.get(id)?.(info));

  beforeEach(() => {
    (
      globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    vi.useFakeTimers();
    pending = new Map();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(
      (_, args) =>
        new Promise((resolve) =>
          pending.set((args as { id: number }).id, resolve),
        ),
    );
    root = createRoot(document.createElement("div"));
  });

  afterEach(() => {
    act(() => root.unmount());
    vi.useRealTimers();
  });

  it("opens with the size already there", async () => {
    await select(book(1));
    expect(seen).toBeNull();
    await answer(1, size(800));
    expect(seen?.book.id).toBe(1);
    expect(seen?.cover?.height).toBe(800);
  });

  it("keeps the previous book until the next one's size is known", async () => {
    await select(book(1));
    await answer(1, size(800));
    await select(book(2));
    expect(seen?.book.id).toBe(1);
    expect(seen?.cover?.height).toBe(800);
    await answer(2, size(1700));
    expect(seen?.book.id).toBe(2);
    expect(seen?.cover?.height).toBe(1700);
  });

  it("ignores a slow answer for a book no longer selected", async () => {
    await select(book(1));
    await select(book(2));
    const stale = pending.get(1);
    await answer(2, size(1700));
    await act(async () => stale?.(size(800)));
    expect(seen?.cover?.height).toBe(1700);
  });

  it("opens anyway when the library is busy, and fills in later", async () => {
    await select(book(1));
    await act(async () => void vi.advanceTimersByTime(200));
    expect(seen?.book.id).toBe(1);
    expect(seen?.cover).toBeNull();
    await answer(1, size(800));
    expect(seen?.cover?.height).toBe(800);
  });

  it("shows edits to the open book without waiting", async () => {
    await select(book(1));
    await answer(1, size(800));
    await select(book(1, "Renamed"));
    expect(seen?.book.title).toBe("Renamed");
    expect(seen?.cover?.height).toBe(800);
  });

  it("forgets the book when nothing is selected", async () => {
    await select(book(1));
    await answer(1, size(800));
    await select(null);
    expect(seen).toBeNull();
    await select(book(2));
    expect(seen).toBeNull();
  });
});
