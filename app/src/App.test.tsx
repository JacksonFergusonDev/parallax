// @vitest-environment jsdom
import { StrictMode, act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import { readCache } from "./readCache";
import type { Book, DeviceBook, DeviceInfo } from "./api";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string) => `asset:${path}`,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(), open: vi.fn() }));

const device = { mount: "/kobo", serial: "test", model: "Kobo" } as DeviceInfo;
const localBook: Book = {
  id: 1,
  title: "Local book",
  authors: ["Author"],
  series: null,
  series_index: null,
  tags: [],
  language: null,
  publisher: null,
  description: null,
  identifiers: [],
  original_name: "book.epub",
  file_size: 100,
  has_cover: true,
  custom_cover: false,
  cover: "/local.jpg",
  added_at: "2026-10-31",
  modified_at: "2026-10-31",
};
const koboBook: DeviceBook = {
  id: "file:///mnt/onboard/book.epub",
  title: "Kobo book",
  author: "Author",
  path: "book.epub",
  sideloaded: true,
  series: null,
  series_number: null,
  read_status: "unread",
  percent_read: 0,
  last_read: null,
  reading_time_secs: null,
  added: null,
  file_size: 100,
  collections: [],
  cover: "/cached-kobo.jpg",
};

let root: Root;
let container: HTMLDivElement;
let connected: DeviceInfo[];
let currentBooks: DeviceBook[];

function panel(name: string) {
  return container.querySelector<HTMLDivElement>(
    `[role="tabpanel"][aria-label="${name}"]`,
  )!;
}
function button(text: string) {
  const result = [
    ...container.querySelectorAll<HTMLButtonElement>("button"),
  ].find((b) => b.textContent?.trim() === text);
  if (!result) throw new Error(`Missing button: ${text}`);
  return result;
}
async function click(element: HTMLElement) {
  await act(async () => element.click());
}
function calls(command: string) {
  return vi.mocked(invoke).mock.calls.filter(([name]) => name === command);
}
async function render() {
  await act(async () =>
    root.render(
      <StrictMode>
        <App />
      </StrictMode>,
    ),
  );
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  localStorage.clear();
  readCache.clear();
  connected = [device];
  currentBooks = [koboBook];
  vi.mocked(invoke)
    .mockReset()
    .mockImplementation(async (command) => {
      switch (command) {
        case "list_devices":
          return connected;
        case "library_list":
          return [localBook];
        case "device_books":
          return currentBooks;
        case "reconcile_device":
          return { series: 0, grouped: 0 };
        default:
          return [];
      }
    });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});

afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("retained app screens", () => {
  it("preloads once and preserves both screens, images and scroll positions across tabs", async () => {
    await render();
    expect(panel("On Kobo").hidden).toBe(true);
    expect(calls("device_books")).toHaveLength(1);
    expect(calls("library_list")).toHaveLength(1);
    const image = panel("On Kobo").querySelector("img");
    const localImage = panel("Library").querySelector("img");
    const scroll = panel("On Kobo").querySelector(".scroll")!;
    scroll.scrollTop = 420;
    const collectionReads = calls("list_collections").length;
    for (let i = 0; i < 3; i++) {
      await click(button("On Kobo"));
      expect(panel("Library").hidden).toBe(true);
      expect(panel("On Kobo").hidden).toBe(false);
      expect(panel("On Kobo").querySelector("img")).toBe(image);
      expect(scroll.scrollTop).toBe(420);
      await click(button("Library"));
      expect(panel("Library").querySelector("img")).toBe(localImage);
    }
    expect(calls("device_books")).toHaveLength(1);
    expect(calls("library_list")).toHaveLength(1);
    expect(calls("list_collections")).toHaveLength(collectionReads);
  });

  it("refreshes the retained Kobo screen after a transfer from Library", async () => {
    await render();
    currentBooks = [{ ...koboBook, title: "Updated device book" }];
    await click(panel("Library").querySelector("button.card")!);
    await click(button("Send 1 to Kobo"));
    expect(calls("device_books")).toHaveLength(2);
    await click(button("On Kobo"));
    expect(panel("On Kobo").textContent).toContain("Updated device book");
    expect(calls("device_books")).toHaveLength(2);
  });

  it("discards the retained device screen on disconnect and reloads on reconnect", async () => {
    await render();
    const oldImage = panel("On Kobo").querySelector("img");
    connected = [];
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(panel("On Kobo")).toBeNull();
    connected = [device];
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(calls("device_books")).toHaveLength(2);
    expect(panel("On Kobo").querySelector("img")).not.toBe(oldImage);
  });

  it("keeps newer data when an earlier read completes after reconciliation", async () => {
    let finishOld!: (books: DeviceBook[]) => void;
    let finishReconcile!: (result: { series: number; grouped: number }) => void;
    const normal = vi.mocked(invoke).getMockImplementation()!;
    let first = true;
    vi.mocked(invoke).mockImplementation((command, args) => {
      if (command === "device_books" && first) {
        first = false;
        return new Promise((resolve) => {
          finishOld = resolve;
        });
      }
      if (command === "reconcile_device") {
        return new Promise((resolve) => {
          finishReconcile = resolve;
        });
      }
      return normal(command, args);
    });
    await render();
    currentBooks = [{ ...koboBook, title: "Fresh metadata" }];
    await act(async () => finishReconcile({ series: 1, grouped: 0 }));
    expect(panel("On Kobo").textContent).toContain("Fresh metadata");
    await act(async () => finishOld([koboBook]));
    expect(panel("On Kobo").textContent).toContain("Fresh metadata");
    expect(panel("On Kobo").textContent).not.toContain("Kobo book");
  });
});
