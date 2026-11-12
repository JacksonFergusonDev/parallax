// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { GoodreadsShelves } from "./GoodreadsShelves";
import type { Book } from "../api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const book = (goodreads?: Book["goodreads"]) => ({ id: 1, goodreads }) as Book;
const match = {
  goodreads_id: "77",
  shelf: "to_read" as const,
  rating: null,
  date_read: null,
  shelves: ["favorites"],
};

describe("GoodreadsShelves", () => {
  let root: Root;
  let host: HTMLDivElement;
  const onChanged = vi.fn();
  const edits = () =>
    vi.mocked(invoke).mock.calls.filter(([c]) => c === "goodreads_edit");
  const show = (b: Book) =>
    act(async () => {
      root.render(<GoodreadsShelves book={b} onChanged={onChanged} />);
    });
  const flush = () =>
    act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

  beforeEach(() => {
    (
      globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    host = document.body.appendChild(document.createElement("div"));
    root = createRoot(host);
    vi.mocked(invoke).mockResolvedValue({});
  });
  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.clearAllMocks();
  });

  it("shows nothing for a book that isn't on your shelves", async () => {
    await show(book());
    expect(host.textContent).toBe("");
  });

  it("shows the status and shelves and saves a new status at once", async () => {
    await show(book(match));
    expect(
      host.querySelector('[role=radio][aria-checked="true"]')?.textContent,
    ).toBe("Want to read");
    expect(host.textContent).toContain("favorites");
    const read = [...host.querySelectorAll("button")].find(
      (b) => b.textContent === "Read",
    )!;
    await act(async () => read.click());
    await flush();
    expect(edits()).toEqual([
      ["goodreads_edit", { goodreadsId: "77", edit: { shelf: "read" } }],
    ]);
    expect(onChanged).toHaveBeenCalled();
  });

  it("removes a shelf", async () => {
    await show(book(match));
    const remove = host.querySelector(
      '[aria-label="Remove from favorites"]',
    ) as HTMLButtonElement;
    await act(async () => remove.click());
    await flush();
    expect(edits()[0][1]).toEqual({
      goodreadsId: "77",
      edit: { remove_shelves: ["favorites"] },
    });
  });
});
