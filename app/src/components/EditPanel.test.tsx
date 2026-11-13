// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EditPanel } from "./EditPanel";
import type { Book, CoverInfo } from "../api";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string) => `asset:${path}`,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const book = (change: Partial<Book> = {}) =>
  ({
    id: 1,
    title: "Dune",
    authors: [],
    contributors: [],
    genres: [],
    tags: [],
    identifiers: [],
    pages: null,
    has_cover: true,
    custom_cover: false,
    cover_upscaled: false,
    modified_at: "2026-01-01",
    ...change,
  }) as unknown as Book;

describe("EditPanel cover details", () => {
  let root: Root;
  let host: HTMLDivElement;

  const show = async (
    info: CoverInfo | null,
    available = true,
    change: Partial<Book> = {},
  ) => {
    await act(async () => {
      root.render(
        <EditPanel
          book={book(change)}
          cover={info}
          canUpscale={available}
          onSave={vi.fn()}
          onChanged={vi.fn()}
          onEnhance={vi.fn()}
          onGoodreadsChanged={vi.fn()}
          onClose={vi.fn()}
        />,
      );
    });
  };

  const openCoverMenu = async () => {
    await act(async () =>
      host
        .querySelector<HTMLButtonElement>(".cover-actions [aria-haspopup]")
        ?.click(),
    );
  };

  beforeEach(() => {
    (
      globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("offers upscaling and says why for a low-resolution cover", async () => {
    await show({ width: 525, height: 791, quality: "low" });
    expect(host.textContent).toContain("525 × 791Low resolution");
    expect(host.textContent).toContain("pixelated");
    expect(host.textContent).toContain(
      "Find a sharper image or upscale this one.",
    );
    expect(host.textContent).not.toContain("Upscale cover…");
    await openCoverMenu();
    expect(host.textContent).toContain("Upscale cover…");
  });

  it("offers upscaling a decent cover, more gently", async () => {
    await show({ width: 900, height: 1400, quality: "decent" });
    expect(host.textContent).toContain("900 × 1400Medium resolution");
    expect(host.textContent).toContain("slightly soft");
    expect(host.textContent).not.toContain("Upscale cover…");
    await openCoverMenu();
    expect(host.textContent).toContain("Upscale cover…");
  });

  it("says nothing more about an excellent cover", async () => {
    await show({ width: 1120, height: 1680, quality: "excellent" });
    expect(host.textContent).toContain("1120 × 1680High resolution");
    expect(host.textContent).not.toContain("e-reader");
    await openCoverMenu();
    expect(host.textContent).not.toContain("Upscale cover…");
  });

  it("doesn't suggest upscaling a cover twice", async () => {
    await show({ width: 400, height: 600, quality: "low" }, true, {
      cover_upscaled: true,
    });
    await openCoverMenu();
    expect(host.textContent).not.toContain("Upscale cover…");
    expect(host.textContent).not.toContain("upscale");
    expect(host.textContent).toContain("Find a sharper image.");
  });

  it("doesn't suggest upscaling when the model isn't in this build", async () => {
    await show({ width: 525, height: 791, quality: "low" }, false);
    await openCoverMenu();
    expect(host.textContent).not.toContain("Upscale cover…");
    expect(host.textContent).not.toContain("upscale");
  });

  it("shows no size without a cover", async () => {
    await show(null, true, { has_cover: false });
    expect(host.querySelector(".cover-size")).toBeNull();
    await openCoverMenu();
    expect(host.textContent).not.toContain("Upscale cover…");
  });
});

describe("EditPanel book details", () => {
  let root: Root;
  let host: HTMLDivElement;

  const show = async (change: Partial<Book>) => {
    await act(async () => {
      root.render(
        <EditPanel
          book={book(change)}
          cover={null}
          canUpscale={false}
          onSave={vi.fn()}
          onChanged={vi.fn()}
          onEnhance={vi.fn()}
          onGoodreadsChanged={vi.fn()}
          onClose={vi.fn()}
        />,
      );
    });
  };
  const fact = (label: string) =>
    [...host.querySelectorAll("dt")].find((dt) => dt.textContent === label)
      ?.nextElementSibling?.textContent;

  beforeEach(() => {
    (
      globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    host.remove();
  });

  it("shows what's known and leaves out the rest", async () => {
    await show({
      published: "1965-08-15",
      pages: 412,
      pages_estimated: true,
      words: 123_456,
      language: "en",
      original_name: "dune.epub",
    });
    expect(fact("Published")).toMatch(/August 1965/);
    expect(fact("Pages")).toBe("About 412");
    expect(fact("Words")).toBe((123_456).toLocaleString());
    expect(fact("Language")).toBe("English");
    expect(fact("File")).toBe("dune.epub");
    expect(fact("Publisher")).toBeUndefined();
    expect(fact("ISBN")).toBeUndefined();
    expect(fact("Last read")).toBeUndefined();
  });

  it("shows a description as text, collapsing long ones", async () => {
    await show({
      description: `<p>First <b>part</b>.</p><p>${"More. ".repeat(80)}</p>`,
    });
    const text = host.querySelector(".description p");
    expect(text?.textContent).toMatch(/^First part\.\nMore\./);
    expect(text?.className).toBe("clamped");

    const toggle = host.querySelector<HTMLButtonElement>(".description button");
    await act(async () => toggle?.click());
    expect(text?.className).toBe("");
    expect(toggle?.textContent).toBe("Show less");
  });
});
