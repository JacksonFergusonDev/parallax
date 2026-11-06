// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { UpscaleReview } from "./UpscaleReview";
import type { Book } from "../api";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string) => `asset:${path}`,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const book = (id: number, title: string) => ({ id, title }) as Book;
const preview = (id: number) => ({
  id,
  run: id * 10,
  before: `/p/${id}-before.png`,
  after: `/p/${id}-after.png`,
  before_size: [300, 450],
  after_size: [1120, 1680],
});

describe("UpscaleReview", () => {
  let root: Root;
  let host: HTMLDivElement;
  const onChanged = vi.fn();
  const onClose = vi.fn();
  const calls = (command: string) =>
    vi.mocked(invoke).mock.calls.filter(([c]) => c === command);

  const show = async (books: Book[]) => {
    await act(async () => {
      root.render(
        <UpscaleReview books={books} onChanged={onChanged} onClose={onClose} />,
      );
    });
  };
  const button = (label: string) =>
    [...host.querySelectorAll("button")].find((b) => b.textContent === label);
  const click = (label: string) =>
    act(async () => {
      button(label)?.click();
    });

  beforeEach(() => {
    (
      globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    vi.mocked(invoke).mockReset();
    onChanged.mockReset();
    onClose.mockReset();
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("changes a book only when its upscaled cover is kept", async () => {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      const id = (args as { id: number } | undefined)?.id;
      if (command === "cover_upscale_preview") return preview(id!);
      if (command === "cover_upscale_apply") return book(id!, "Updated");
      return null;
    });
    await show([book(1, "Dune"), book(2, "Emma")]);

    expect(host.textContent).toContain("Dune · 1 of 2");
    await click("Keep original");
    expect(calls("cover_upscale_apply")).toEqual([]);
    expect(host.textContent).toContain("Emma · 2 of 2");

    await click("Use upscaled");
    expect(calls("cover_upscale_apply")).toEqual([
      ["cover_upscale_apply", { id: 2, run: 20 }],
    ]);
    expect(onChanged).toHaveBeenCalledOnce();
    expect(host.textContent).toContain("1 cover upscaled.");
  });

  it("skips covers that are already full size", async () => {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      const id = (args as { id: number } | undefined)?.id;
      if (command === "cover_upscale_preview")
        return id === 1 ? null : preview(id!);
      return null;
    });
    await show([book(1, "Dune"), book(2, "Emma")]);

    expect(host.textContent).toContain("Emma · 1 of 1");
    await click("Keep original");
    expect(host.textContent).toContain("0 covers upscaled.");
    expect(host.textContent).not.toContain("full size");
  });

  it("closes itself once a single cover is decided", async () => {
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      const id = (args as { id: number } | undefined)?.id;
      if (command === "cover_upscale_preview") return preview(id!);
      return null;
    });
    await show([book(1, "Dune")]);
    expect(onClose).not.toHaveBeenCalled();
    await click("Keep original");
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("closes without a word if a single cover needs nothing", async () => {
    vi.mocked(invoke).mockResolvedValue(null);
    await show([book(1, "Dune")]);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("stops the job and stops asking when closed", async () => {
    let finish: (value: unknown) => void = () => {};
    vi.mocked(invoke).mockImplementation((command) =>
      command === "cover_upscale_preview"
        ? new Promise((resolve) => (finish = resolve))
        : Promise.resolve(null),
    );
    await show([book(1, "Dune"), book(2, "Emma")]);

    act(() => root.unmount());
    expect(calls("cover_upscale_cancel")).toHaveLength(1);

    await act(async () => finish(preview(1)));
    expect(calls("cover_upscale_preview")).toHaveLength(1);
    root = createRoot(host);
  });

  it("shows why upscaling failed", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "cover_upscale_preview") throw "the model failed";
      return null;
    });
    await show([book(1, "Dune")]);
    expect(host.textContent).toContain("the model failed");
    expect(button("Cancel")).toBeDefined();
  });
});
