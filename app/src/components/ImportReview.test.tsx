// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ImportReview } from "./ImportReview";
import type { Book, Issue, Proposal } from "../api";
import { readCache } from "../readCache";
import type { ReviewBatch } from "../importReview";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string) => `asset:${path}`,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(async () => true) }));

const book = (id: number, title: string) =>
  ({
    id,
    title,
    authors: ["An Author"],
    tags: [],
    series: null,
    series_index: null,
    has_cover: false,
    cover: null,
  }) as unknown as Book;

const batch = (...findings: [Issue, number][]): ReviewBatch => ({
  ids: [...new Set(findings.map(([, id]) => id))],
  failed: [],
  review: {
    findings: findings.map(([issue, book_id]) => ({
      issue,
      book_id,
      detail: "",
    })),
    duplicates: [],
    bulk: false,
  },
});

const fantasy = (book_id: number): Proposal => ({
  book_id,
  series: null,
  genres: [{ name: "Fantasy", sources: ["Open Library"], confident: true }],
  problems: [],
});

describe("ImportReview", () => {
  let root: Root;
  let host: HTMLDivElement;
  let proposals: Proposal[];
  const onChanged = vi.fn();
  const onClose = vi.fn();
  const calls = (command: string) =>
    vi.mocked(invoke).mock.calls.filter(([c]) => c === command);
  const button = (text: string) =>
    [...host.querySelectorAll("button")].find((b) => b.textContent === text);
  const click = (text: string) =>
    act(async () => {
      const target = button(text);
      if (!target)
        throw new Error(`no button "${text}" in ${host.textContent}`);
      target.click();
    });

  const show = (b: ReviewBatch, books: Book[]) =>
    act(async () => {
      root.render(
        <ImportReview
          batch={b}
          books={new Map(books.map((x) => [x.id, x]))}
          mount={null}
          canUpscale={false}
          onKobo={() => false}
          onChanged={onChanged}
          onClose={onClose}
        />,
      );
    });

  beforeEach(() => {
    (globalThis as Record<string, unknown>).IS_REACT_ACT_ENVIRONMENT = true;
    proposals = [];
    readCache.clear();
    vi.clearAllMocks();
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      switch (command) {
        case "enhance_propose":
          return proposals;
        case "enhance_apply":
          return (args as { accepted: { book_id: number }[] }).accepted.map(
            (a) => book(a.book_id, "Saved"),
          );
        case "library_update":
          return book((args as { id: number }).id, "Saved");
        case "cover_search":
          return [];
        default:
          return null;
      }
    });
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("says what it found, and saves nothing if kept as is", async () => {
    proposals = [fantasy(1)];
    await show(batch(["no_genre", 1]), [book(1, "One")]);
    expect(host.textContent).toContain("1 book added");
    expect(host.textContent).toContain("Found genres for 1");
    await click("Keep as is");
    expect(onClose).toHaveBeenCalled();
    expect(calls("enhance_apply")).toHaveLength(0);
  });

  it("saves ticked proposals on leaving the step, then says so", async () => {
    proposals = [fantasy(1)];
    await show(batch(["no_genre", 1]), [book(1, "One")]);
    await click("Review");
    expect(host.textContent).toContain("Step 1 of 1");
    await click("Save and continue");
    expect(calls("enhance_apply")[0][1]).toEqual({
      accepted: [{ book_id: 1, series: null, genres: ["Fantasy"] }],
    });
    expect(onChanged).toHaveBeenCalled();
    expect(host.textContent).toContain("Updated the details of 1 book");
    expect(onClose).not.toHaveBeenCalled();
  });

  it("skips a step without saving and ends if nothing changed", async () => {
    proposals = [fantasy(1)];
    await show(batch(["no_genre", 1]), [book(1, "One")]);
    await click("Review");
    await click("Skip this step");
    expect(calls("enhance_apply")).toHaveLength(0);
    expect(onClose).toHaveBeenCalled();
  });

  it("only offers Done when nothing can be acted on", async () => {
    await show(batch(["drm", 1]), [book(1, "One")]);
    expect(host.textContent).toContain("1 book has DRM");
    expect(button("Review")).toBeUndefined();
    expect(button("Done")).toBeDefined();
  });

  it("removes a book the user gives up on and forgets its findings", async () => {
    await show(batch(["damaged", 1], ["drm", 2]), [
      book(1, "One"),
      book(2, "Two"),
    ]);
    const remove = [...host.querySelectorAll("button")].find(
      (b) => b.textContent === "Remove",
    );
    await act(async () => remove?.click());
    expect(calls("library_remove")[0][1]).toEqual({ ids: [1] });
    expect(host.textContent).not.toContain("damaged");
    expect(host.textContent).toContain("1 book added");
  });

  it("searches covers again after a title is corrected", async () => {
    await show(batch(["title_from_file", 1], ["no_cover", 1]), [
      book(1, "my_scan"),
    ]);
    expect(calls("cover_search")).toHaveLength(1);
    await click("Review");
    const input = host.querySelector("input:not([type])") as HTMLInputElement;
    await act(async () => {
      const set = Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        "value",
      )?.set;
      set?.call(input, "A Real Title");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    await click("Save and continue");
    expect(calls("library_update")).toHaveLength(1);
    expect(calls("cover_search")).toHaveLength(2);
  });

  it("doesn't close on a click outside", async () => {
    await show(batch(["no_genre", 1]), [book(1, "One")]);
    await act(async () =>
      (host.querySelector(".modal-backdrop") as HTMLElement).click(),
    );
    expect(onClose).not.toHaveBeenCalled();
  });
});
