// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ImportReview } from "./ImportReview";
import {
  removeBooks,
  type Book,
  type Issue,
  type Proposal,
  type SendOutcome,
} from "../api";
import { readCache } from "../readCache";
import type { ReviewBatch } from "../importReview";
import type { SendStatus } from "../library";
import type { BatchTransferProgress } from "../transferEstimate";

vi.mock("@tauri-apps/api/core", () => ({
  Channel: class<T> {
    constructor(public onmessage: (message: T) => void) {}
  },
  invoke: vi.fn(),
  convertFileSrc: (path: string) => `asset:${path}`,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

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
  const send = vi.fn(async (ids: number[]) =>
    ids.map(
      (id) =>
        ({
          source: String(id),
          book_id: id,
          result: id === 9 ? { Err: "no space" } : { Ok: {} },
        }) as SendOutcome,
    ),
  );
  const koboFor = { name: "Kobo Libra 2", send };
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

  const show = (
    b: ReviewBatch,
    books: Book[],
    kobo: { name: string; send: typeof send } | null = null,
    statuses: Record<number, SendStatus> = {},
    batchTransfer: BatchTransferProgress | null = null,
  ) =>
    act(async () => {
      root.render(
        <ImportReview
          batch={b}
          books={new Map(books.map((x) => [x.id, x]))}
          mount={null}
          canUpscale={false}
          onKobo={() => false}
          kobo={kobo}
          statuses={statuses}
          batchTransfer={batchTransfer}
          prepareRemove={async (ids) => ({
            starting: "Saving a library snapshot…",
            run: (report) => removeBooks(ids, report),
          })}
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
    expect(host.textContent).toContain("Book details");
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
    expect(calls("library_remove")[0][1]).toMatchObject({ ids: [1] });
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

  it("labels the current and new covers and selects by clicking", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "enhance_propose") return [];
      if (command === "cover_info")
        return { width: 300, height: 450, quality: "low" };
      if (command === "cover_search")
        return [{ source: "Apple Books", url: "u", preview_url: "p" }];
      return null;
    });
    const measured = vi
      .spyOn(readCache, "get")
      .mockImplementation(((key: unknown[], load: () => Promise<unknown>) =>
        key[0] === "image-size"
          ? Promise.resolve([1000, 1500])
          : load()) as never);
    await show(batch(["small_cover", 1]), [book(1, "One")]);
    await click("Review");
    const options = () =>
      [...host.querySelectorAll(".review-cover-option")].map(
        (o) => o.textContent,
      );
    expect(options()[0]).toMatch(/^Current cover.*300×450/);
    expect(options()[1]).toMatch(/^New cover.*Apple Books · 1000×1500/);
    expect(options()[1]).toContain("Selected");
    await act(async () =>
      (host.querySelector(".review-cover-option") as HTMLElement).click(),
    );
    expect(options()[0]).toContain("Selected");
    await click("Save and continue");
    expect(calls("cover_set_url")).toHaveLength(0);
    measured.mockRestore();
  });

  it("goes straight to sending when there's nothing to fix", async () => {
    await show(
      { ids: [1, 2], failed: [], review: batch().review },
      [book(1, "One"), book(2, "Two")],
      koboFor,
    );
    expect(host.textContent).toContain("Send to your Kobo?");
    expect(calls("enhance_propose")).toHaveLength(0);
    await click("Send 2 books");
    expect(send).toHaveBeenCalledWith([1, 2]);
    expect(host.textContent).toContain("Sent 2 books to your Kobo");
    expect(host.textContent).toContain("Eject your Kobo");
  });

  it("keeps batch progress visible as the active volume switches from pages to copying", async () => {
    const review = { ids: [1, 2], failed: [], review: batch().review };
    const books = [book(1, "One"), book(2, "Two")];
    const progress = (label: string) =>
      host.querySelector<HTMLProgressElement>(
        `progress[aria-label="${label}"]`,
      );
    let resolveSend!: (outcomes: SendOutcome[]) => void;
    send.mockReturnValueOnce(new Promise((resolve) => (resolveSend = resolve)));
    await show(review, books, koboFor);
    await click("Send 2 books");

    await show(
      review,
      books,
      koboFor,
      {
        1: { stage: "processing_comic", page: 50, pages: 100 },
        2: { stage: "queued" },
      },
      {
        completed: 0,
        total: 2,
        value: 0.4,
        secondsLeft: 19,
        activeComic: {
          stage: "processing_comic",
          file: "One",
          index: 1,
          total: 2,
          page: 50,
          pages: 100,
        },
      },
    );
    expect(progress("Kobo transfer batch")?.value).toBe(0.4);
    expect(host.textContent).toContain("About 20s left");
    expect(progress("Comic pages processed")?.position).toBe(0.5);

    await show(
      review,
      books,
      koboFor,
      {
        1: { stage: "copying", bytes: 25, totalBytes: 100 },
        2: { stage: "queued" },
      },
      {
        completed: 0,
        total: 2,
        value: 0.85,
        secondsLeft: 12,
        activeComic: null,
      },
    );
    expect(progress("Comic pages processed")).toBeNull();
    expect(progress("Transfer to Kobo")?.position).toBe(0.25);
    expect(progress("Kobo transfer batch")?.value).toBe(0.85);

    await act(async () => resolveSend([]));
    expect(host.querySelector("progress")).toBeNull();
  });

  it("still offers sending after keeping books as they are", async () => {
    await show(batch(["no_genre", 1]), [book(1, "One")], koboFor);
    await click("Keep as is");
    expect(onClose).not.toHaveBeenCalled();
    expect(host.textContent).toContain("Send to your Kobo?");
    await click("Not now");
    expect(send).not.toHaveBeenCalled();
    expect(onClose).toHaveBeenCalled();
  });

  it("doesn't tick a damaged book, and says which failed", async () => {
    await show(
      batch(["damaged", 1], ["no_author", 9]),
      [book(1, "One"), book(9, "Nine")],
      koboFor,
    );
    await click("Review");
    await click("Continue");
    expect(host.textContent).toContain("damaged");
    await click("Send 1 book");
    expect(send).toHaveBeenCalledWith([9]);
    expect(host.textContent).toContain("1 book couldn't be sent");
    expect(host.textContent).toContain("Nine");
  });

  it("doesn't offer sending without a Kobo", async () => {
    await show(batch(["drm", 1]), [book(1, "One")]);
    expect(button("Continue")).toBeUndefined();
    expect(button("Done")).toBeDefined();
  });

  it("doesn't close on a click outside", async () => {
    await show(batch(["no_genre", 1]), [book(1, "One")]);
    await act(async () =>
      (host.querySelector(".modal-backdrop") as HTMLElement).click(),
    );
    expect(onClose).not.toHaveBeenCalled();
  });
});
