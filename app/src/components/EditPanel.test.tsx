// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { EditPanel } from "./EditPanel";
import { readCache } from "../readCache";
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
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "cover_info") return info;
      if (command === "cover_upscale_available") return available;
      return null;
    });
    await act(async () => {
      root.render(
        <EditPanel
          book={book(change)}
          onSave={vi.fn()}
          onChanged={vi.fn()}
          onEnhance={vi.fn()}
          onClose={vi.fn()}
        />,
      );
    });
  };

  beforeEach(() => {
    (
      globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    vi.mocked(invoke).mockReset();
    readCache.clear();
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
    expect(host.textContent).toContain("525×791 (low)");
    expect(host.textContent).toContain("pixellated");
    expect(host.textContent).toContain("Upscaling or a sharper cover");
    expect(host.textContent).toContain("Upscale cover…");
  });

  it("offers upscaling a decent cover, more gently", async () => {
    await show({ width: 900, height: 1400, quality: "decent" });
    expect(host.textContent).toContain("900×1400 (decent)");
    expect(host.textContent).toContain("slightly soft");
    expect(host.textContent).toContain("Upscale cover…");
  });

  it("says nothing more about an excellent cover", async () => {
    await show({ width: 1120, height: 1680, quality: "excellent" });
    expect(host.textContent).toContain("1120×1680 (excellent)");
    expect(host.textContent).not.toContain("e-reader");
    expect(host.textContent).not.toContain("Upscale cover…");
  });

  it("doesn't suggest upscaling a cover twice", async () => {
    await show({ width: 400, height: 600, quality: "low" }, true, {
      cover_upscaled: true,
    });
    expect(host.textContent).not.toContain("Upscale cover…");
    expect(host.textContent).not.toContain("Upscaling");
    expect(host.textContent).toContain("A sharper cover would help.");
  });

  it("doesn't suggest upscaling when the model isn't in this build", async () => {
    await show({ width: 525, height: 791, quality: "low" }, false);
    expect(host.textContent).not.toContain("Upscale cover…");
    expect(host.textContent).not.toContain("Upscaling");
  });

  it("shows no size without a cover", async () => {
    await show(null, true, { has_cover: false });
    expect(host.querySelector(".cover-size")).toBeNull();
    expect(host.textContent).not.toContain("Upscale cover…");
  });
});
