// @vitest-environment jsdom
import { StrictMode, act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke, isTauri } from "@tauri-apps/api/core";
import App from "./App";
import { readCache } from "./readCache";
import type { Book, DeviceBook, DeviceInfo } from "./api";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: vi.fn(() => false),
  convertFileSrc: (path: string) => `asset:${path}`,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ onDragDropEvent: async () => () => {} }),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(), open: vi.fn() }));

const device: DeviceInfo = {
  kind: "kobo",
  mount: "/kobo",
  serial: "test",
  model: "Kobo",
  firmware: "4.38.23697",
  capabilities: {
    native_collections: true,
    series: true,
    reading_progress: true,
    reading_time: true,
  },
  read_only: null,
};
const localBook: Book = {
  id: 1,
  title: "Local book",
  authors: ["Author"],
  series: null,
  series_index: null,
  tags: [],
  language: null,
  publisher: null,
  published: null,
  description: null,
  identifiers: [],
  words: null,
  pages: null,
  pages_estimated: true,
  original_name: "book.epub",
  file_size: 100,
  has_cover: true,
  custom_cover: false,
  cover_upscaled: false,
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
  vi.mocked(isTauri).mockReturnValue(false);
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
        case "device_history":
          return {
            device: null,
            books: [],
            log: [],
            problems: [],
            changed: [],
            missing: [],
          };
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

describe("read-only Kobos", () => {
  it("explains why and offers no changes", async () => {
    connected = [{ ...device, read_only: "firmware 5.1.0 is too new" }];
    await render();
    await click(panel("Library").querySelector("button.card")!);
    expect(button("Send 1 to Kobo").disabled).toBe(true);
    await click(button("On Kobo"));
    expect(panel("On Kobo").textContent).toContain(
      "only reads this Kobo: firmware 5.1.0 is too new",
    );
    await click(panel("On Kobo").querySelector("button.card")!);
    expect(button("Remove 1 from Kobo").disabled).toBe(true);
    const menus = [...panel("On Kobo").querySelectorAll("button")].filter((b) =>
      /^(Add to collection|Mark as)/.test(b.textContent ?? ""),
    );
    expect(menus.map((b) => b.disabled)).toEqual([true, true]);
    expect(button("Add 1 to library").disabled).toBe(false);
  });
});

describe("selection and collections", () => {
  it("selects one book per click and more with ⌘ in both views", async () => {
    currentBooks = [
      koboBook,
      { ...koboBook, id: "file:///mnt/onboard/b.epub" },
    ];
    await render();
    const pressed = (name: string) =>
      [...panel(name).querySelectorAll("button.card")].filter(
        (c) => c.getAttribute("aria-pressed") === "true",
      ).length;
    const cards = () =>
      panel("On Kobo").querySelectorAll<HTMLButtonElement>("button.card");
    await click(button("On Kobo"));
    await click(cards()[0]);
    await click(cards()[1]);
    expect(pressed("On Kobo")).toBe(1);
    await act(async () =>
      cards()[0].dispatchEvent(
        new MouseEvent("click", { bubbles: true, metaKey: true }),
      ),
    );
    expect(pressed("On Kobo")).toBe(2);
    expect(panel("On Kobo").textContent).toContain("⌘-click to select several");
  });

  it("adds Library books already on the Kobo to a collection", async () => {
    const normal = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "books_on_device")
        return [
          {
            book_id: 1,
            device_path: "book.epub",
            content_id: "file:///mnt/onboard/book.epub",
            sent_at: "2026-11-01",
            stale: false,
          },
        ];
      if (command === "list_collections")
        return [{ name: "Favourites", book_count: 0 }];
      return normal(command, args);
    });
    await render();
    await click(panel("Library").querySelector("button.card")!);
    await click(button("Add 1 to collection ▾"));
    await click(
      panel("Library").querySelector<HTMLButtonElement>(
        '.menu [role="menuitem"]',
      )!,
    );
    expect(calls("change_collections").at(-1)?.[1]).toEqual({
      mount: "/kobo",
      changes: [
        {
          kind: "add",
          name: "Favourites",
          content_ids: ["file:///mnt/onboard/book.epub"],
        },
      ],
    });
  });
});

describe("reading stats", () => {
  it("reads fresh stats from a connected Kobo and saved ones without it", async () => {
    await render();
    await click(button("Reading"));
    expect(calls("reading_stats").at(-1)?.[1]).toEqual({ mount: "/kobo" });
    expect(panel("Reading").textContent).toContain("No reading stats yet");
    connected = [];
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(calls("reading_stats").at(-1)?.[1]).toEqual({ mount: null });
    expect(panel("Reading")).not.toBeNull();
  });
});

describe("device badge", () => {
  const about = () => document.querySelector(".modal.about");
  const badge = () => container.querySelector<HTMLElement>("button.badge")!;

  it("opens About this Kobo from any tab, while that tab stays open", async () => {
    await render();
    await click(button("Reading"));
    await click(badge());
    expect(about()?.textContent).toContain("About this Kobo");
    expect(button("Reading").getAttribute("aria-selected")).toBe("true");
  });

  it("doesn't reopen About after the Kobo is unplugged and returns", async () => {
    await render();
    await click(badge());
    expect(about()).not.toBeNull();
    connected = [];
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(about()).toBeNull();
    connected = [device];
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(about()).toBeNull();
  });

  it("isn't a button with no Kobo connected", async () => {
    connected = [];
    await render();
    expect(container.querySelector("button.badge")).toBeNull();
    expect(container.querySelector(".badge")?.textContent).toContain(
      "No device",
    );
  });
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

  it("keeps Library usable without piling up polls while device detection stalls", async () => {
    let finishDetection!: (devices: DeviceInfo[]) => void;
    const normal = vi.mocked(invoke).getMockImplementation()!;
    let first = true;
    vi.mocked(invoke).mockImplementation((command, args) => {
      if (command === "list_devices" && first) {
        first = false;
        return new Promise((resolve) => {
          finishDetection = resolve;
        });
      }
      return normal(command, args);
    });
    await render();
    expect(panel("Library").textContent).toContain("Local book");
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10_000);
    });
    expect(calls("list_devices")).toHaveLength(1);
    await click(panel("Library").querySelector("button.card")!);
    expect(
      panel("Library")
        .querySelector("button.card")
        ?.getAttribute("aria-pressed"),
    ).toBe("true");
    await act(async () => finishDetection([device]));
    expect(panel("On Kobo")).not.toBeNull();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(calls("list_devices")).toHaveLength(2);
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

it("reveals the native window once the initial library has settled and painted", async () => {
  vi.mocked(isTauri).mockReturnValue(true);
  Object.defineProperty(document, "fonts", {
    configurable: true,
    value: { ready: Promise.resolve() },
  });
  let finish!: (books: Book[]) => void;
  const initial = new Promise<Book[]>((resolve) => {
    finish = resolve;
  });
  const backend = vi.mocked(invoke).getMockImplementation()!;
  vi.mocked(invoke).mockImplementation((command, args, options) =>
    command === "library_list"
      ? (initial as ReturnType<typeof invoke>)
      : backend(command, args, options),
  );
  await render();
  await act(async () => {
    await vi.advanceTimersByTimeAsync(500);
  });
  expect(calls("show_main_window")).toHaveLength(0);
  await act(async () => {
    finish([localBook]);
  });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(150);
  });
  expect(calls("show_main_window")).toHaveLength(1);
  expect(panel("Library").textContent).toContain("Local book");
});
