// @vitest-environment jsdom
import { StrictMode, act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ask, message } from "@tauri-apps/plugin-dialog";
import { invoke, isTauri } from "@tauri-apps/api/core";
import App from "./App";
import { readCache } from "./readCache";
import type {
  Book,
  BookProgress,
  DeviceBook,
  DeviceInfo,
  Progress,
  RemoveOutcome,
  SendOutcome,
} from "./api";

vi.mock("@tauri-apps/api/core", () => ({
  Channel: class<T> {
    constructor(public onmessage: (message: T) => void) {}
  },
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
vi.mock("@tauri-apps/plugin-dialog", () => ({
  ask: vi.fn(),
  message: vi.fn(),
  open: vi.fn(),
}));

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
  screen: { width: 1264, height: 1680, colour: false, source: "model" },
  read_only: null,
  tested: true,
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
  format: "epub",
  kind: "book",
  direction: "ltr",
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
  vi.mocked(ask).mockReset();
  vi.mocked(message).mockReset();
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

describe("removing from the library", () => {
  it("asks only about the library when the Kobo is unplugged", async () => {
    connected = [];
    vi.mocked(ask).mockResolvedValueOnce(true);
    await render();
    await click(panel("Library").querySelector("button.card")!);
    await click(button("Remove"));
    expect(ask).toHaveBeenCalledWith(
      "Remove 1 book from your library?",
      expect.objectContaining({ okLabel: "Remove", cancelLabel: "Cancel" }),
    );
    expect(message).not.toHaveBeenCalled();
    expect(calls("library_remove")).toHaveLength(1);
    expect(calls("remove_from_device")).toHaveLength(0);
  });

  it.each([
    { choice: "Library Only", deviceRemovals: 0, libraryRemovals: 1 },
    { choice: "Cancel", deviceRemovals: 0, libraryRemovals: 0 },
    { choice: "Library and Kobo", deviceRemovals: 1, libraryRemovals: 1 },
    { choice: "Library and Kobo", deviceRemovals: 1, libraryRemovals: 0 },
  ])(
    "handles $choice with $libraryRemovals library writes and $deviceRemovals Kobo writes",
    async ({ choice, deviceRemovals, libraryRemovals }) => {
      const other = { ...localBook, id: 2, title: "Library-only book" };
      const mutations: string[] = [];
      const normal = vi.mocked(invoke).getMockImplementation()!;
      vi.mocked(invoke).mockImplementation(async (command, args) => {
        if (command === "library_list") return [localBook, other];
        if (command === "books_on_device")
          return [
            {
              book_id: localBook.id,
              device_path: koboBook.path,
              content_id: koboBook.id,
              sent_at: "2026-11-01",
              stale: false,
            },
          ];
        if (command === "remove_from_device") {
          mutations.push(command);
          return [
            {
              content_id: koboBook.id,
              book_id: localBook.id,
              result: libraryRemovals
                ? { Ok: null }
                : { Err: "Device unavailable" },
            },
          ];
        }
        if (command === "library_remove") mutations.push(command);
        return normal(command, args);
      });
      vi.mocked(message).mockResolvedValueOnce(choice);
      await render();
      const cards =
        panel("Library").querySelectorAll<HTMLButtonElement>("button.card");
      await click(cards[0]);
      await act(async () => {
        cards[1].dispatchEvent(
          new MouseEvent("click", { bubbles: true, metaKey: true }),
        );
      });
      await click(button("Remove"));
      expect(message).toHaveBeenCalledWith(
        expect.stringContaining(
          "Remove 2 books from your library? One selected book has a copy on your Kobo.",
        ),
        expect.objectContaining({
          buttons: {
            yes: "Library Only",
            no: "Library and Kobo",
            cancel: "Cancel",
          },
        }),
      );
      expect(calls("remove_from_device")).toHaveLength(deviceRemovals);
      expect(calls("library_remove")).toHaveLength(libraryRemovals);
      if (deviceRemovals)
        expect(calls("remove_from_device")[0][1]).toMatchObject({
          contentIds: [koboBook.id],
          expectedSerial: device.serial,
        });
      if (libraryRemovals)
        expect(calls("library_remove")[0][1]).toMatchObject({
          ids: expect.arrayContaining([localBook.id, other.id]),
        });
      if (deviceRemovals && libraryRemovals)
        expect(mutations).toEqual(["remove_from_device", "library_remove"]);
      if (deviceRemovals && !libraryRemovals)
        expect(panel("Library").textContent).toContain(
          "Your library books were kept.",
        );
    },
  );
});

describe("book operation progress", () => {
  it("fills the Kobo removal bar during each book after backup", async () => {
    const other = { ...koboBook, id: "file:///mnt/onboard/second.epub" };
    currentBooks = [koboBook, other];
    vi.mocked(ask).mockResolvedValueOnce(true);
    let finish!: (result: RemoveOutcome[]) => void;
    const pending = new Promise<RemoveOutcome[]>((resolve) => {
      finish = resolve;
    });
    const normal = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (command, args) =>
      command === "remove_from_device" ? pending : normal(command, args),
    );

    await render();
    await click(button("On Kobo"));
    const view = panel("On Kobo");
    const cards = view.querySelectorAll<HTMLButtonElement>("button.card");
    await click(cards[0]);
    await act(async () => {
      cards[1].dispatchEvent(
        new MouseEvent("click", { bubbles: true, metaKey: true }),
      );
    });
    await click(button("Remove 2 from Kobo"));
    const progress = () => view.querySelector<HTMLProgressElement>("progress")!;
    expect(view.textContent).toContain("Backing up Kobo…");
    expect(progress().hasAttribute("value")).toBe(false);
    const { progress: channel } = calls("remove_from_device")[0][1] as {
      progress: { onmessage: (message: BookProgress) => void };
    };
    const update = async (completed: number, file_fraction: number) =>
      act(async () =>
        channel.onmessage({ completed, total: 2, title: null, file_fraction }),
      );
    await update(0, 0);
    const bar = view.querySelector('[role="progressbar"]')!;
    expect(bar.getAttribute("aria-valuenow")).toBe("0");
    expect(view.textContent).toContain("Removing 1 of 2");
    await update(0, 0.5);
    expect(bar.getAttribute("aria-valuenow")).toBe("0.5");
    await update(1, 0);
    expect(view.textContent).toContain("Removing 2 of 2");
    expect(bar.getAttribute("aria-valuenow")).toBe("1");
    await update(1, 0.25);
    expect(bar.getAttribute("aria-valuenow")).toBe("1.25");
    await update(2, 0);
    expect(bar.getAttribute("aria-valuenow")).toBe("2");

    currentBooks = [];
    await act(async () =>
      finish(
        [koboBook, other].map((b) => ({
          content_id: b.id,
          result: { Ok: null },
        })),
      ),
    );
    expect(view.querySelector('[role="progressbar"]')).toBeNull();
    expect(view.textContent).toContain("Removed 2 books");
  });

  it("replaces transfer progress with an import badge until the Kobo imports the book", async () => {
    let finish!: (result: SendOutcome[]) => void;
    const pending = new Promise<SendOutcome[]>((resolve) => {
      finish = resolve;
    });
    const normal = vi.mocked(invoke).getMockImplementation()!;
    let copied = false;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "send_books") return pending;
      if (command === "books_on_device" && copied)
        return [
          {
            book_id: localBook.id,
            device_path: koboBook.path,
            content_id: koboBook.id,
            sent_at: "2026-11-01",
            stale: false,
          },
        ];
      if (command === "eject_device") connected = [];
      return normal(command, args);
    });

    await render();
    const card =
      panel("Library").querySelector<HTMLButtonElement>("button.card")!;
    await click(card);
    await click(button("Send 1 to Kobo"));
    const { progress: channel } = calls("send_books")[0][1] as {
      progress: { onmessage: (message: Progress) => void };
    };
    const base = { index: 1, total: 1, file: "Local book" };
    await act(async () => channel.onmessage({ stage: "converting", ...base }));
    const sheen = card.querySelector(".cover-sheen");
    expect(sheen).not.toBeNull();
    copied = true;
    currentBooks = [
      { ...koboBook, title: localBook.title, pending_import: true },
    ];
    await act(async () =>
      channel.onmessage({
        stage: "copying",
        ...base,
        bytes: 25,
        total_bytes: 100,
      }),
    );
    expect(card.querySelector(".cover-sheen")).toBe(sheen);
    const progress =
      card.querySelector<HTMLProgressElement>(".cover progress")!;
    expect([progress.value, progress.max]).toEqual([25, 100]);
    expect(card.textContent).toContain("Transferring 25%");

    await act(async () =>
      finish([
        {
          source: "Local book",
          book_id: localBook.id,
          result: {
            Ok: {
              device_path: "KEpub/book.epub",
              content_id: koboBook.id,
              bytes: 100,
              skipped_pages: [],
            },
          },
        },
      ]),
    );
    expect(card.querySelector("progress")).toBeNull();
    expect(card.querySelector(".cover-sheen")).toBeNull();
    expect(card.textContent).toContain("Eject to import");
    expect(button("On Kobo 1")).toBeTruthy();

    await click(button("On Kobo"));
    const view = panel("On Kobo");
    expect(view.querySelector(".card")?.textContent).toContain(
      "Eject to import",
    );
    await click(view.querySelector<HTMLButtonElement>(".card")!);
    expect(button("Mark as ▾").disabled).toBe(true);
    expect(button("Remove 1 from Kobo").disabled).toBe(false);
    await click(
      [...view.querySelectorAll("button")].find(
        (b) => b.textContent === "Eject Kobo",
      )!,
    );
    expect(panel("On Kobo")).toBeNull();
    expect(card.textContent).not.toContain("Eject to import");

    currentBooks = [{ ...koboBook, title: localBook.title }];
    connected = [device];
    await act(async () => vi.advanceTimersByTimeAsync(2000));
    expect(panel("On Kobo").querySelector(".card")?.textContent).not.toContain(
      "Eject to import",
    );
    expect(card.querySelector(".on-device")?.textContent).toBe("On Kobo");
  });

  it("marks a removed copy in Library and counts it as off the Kobo", async () => {
    const normal = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "books_on_device")
        return currentBooks.length
          ? [
              {
                book_id: localBook.id,
                device_path: koboBook.path,
                content_id: koboBook.id,
                sent_at: "2026-11-01",
                stale: false,
              },
            ]
          : [];
      if (command === "remove_from_device") {
        currentBooks = [];
        return [
          {
            content_id: koboBook.id,
            book_id: localBook.id,
            result: { Ok: null },
          },
        ];
      }
      return normal(command, args);
    });
    vi.mocked(ask).mockResolvedValueOnce(true);
    await render();
    await click(button("On Kobo"));
    const view = panel("On Kobo");
    await click(view.querySelector<HTMLButtonElement>(".card")!);
    await click(button("Remove 1 from Kobo"));
    expect(view.querySelector(".card")).toBeNull();
    await click(button("Library"));
    await click(button("Not on Kobo 1"));
    expect(panel("Library").textContent).toContain("Removed 1 book from Kobo");
    const card = panel("Library").querySelector<HTMLButtonElement>(".card")!;
    expect(card.querySelector(".on-device")?.textContent).toBe(
      "Removed from Kobo",
    );
    await click(card);
    expect(button("Send 1 to Kobo").disabled).toBe(false);
  });

  it("shows an eject badge on an updated copy in both views", async () => {
    let stale = true;
    const normal = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "books_on_device")
        return [
          {
            book_id: localBook.id,
            device_path: koboBook.path,
            content_id: koboBook.id,
            sent_at: "2026-11-01",
            stale,
            original_comic: false,
          },
        ];
      if (command === "update_books") {
        stale = false;
        return [
          {
            source: localBook.title,
            book_id: localBook.id,
            result: {
              Ok: {
                device_path: koboBook.path,
                content_id: koboBook.id,
                bytes: 100,
                skipped_pages: [],
                details_only: false,
              },
            },
          },
        ];
      }
      return normal(command, args);
    });
    await render();
    const card = panel("Library").querySelector<HTMLButtonElement>(".card")!;
    await click(card);
    await click(button("Update 1 on Kobo"));
    expect(card.querySelector(".on-device")?.textContent).toBe(
      "Eject to see update",
    );
    expect(button("On Kobo 1")).toBeTruthy();
    expect(panel("Library").textContent).not.toContain("Update available");
    await click(button("On Kobo"));
    expect(panel("On Kobo").querySelector(".on-device")?.textContent).toBe(
      "Eject to see update",
    );
  });
});

describe("comics on the Kobo as their original CBZ", () => {
  it("says what updating changes and how to get a prepared copy", async () => {
    const normal = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "library_list")
        return [{ ...localBook, title: "Storm v02", format: "cbz" }];
      if (command === "books_on_device")
        return [
          {
            book_id: localBook.id,
            device_path: "Comics/Storm v02.cbz",
            content_id: "file:///mnt/onboard/Comics/Storm v02.cbz",
            sent_at: "2026-11-01",
            stale: true,
            original_comic: true,
          },
        ];
      return normal(command, args);
    });
    vi.mocked(ask).mockResolvedValueOnce(false);
    await render();
    await click(panel("Library").querySelector<HTMLButtonElement>(".card")!);
    await click(button("Update 1 on Kobo"));
    const [text, options] = vi.mocked(ask).mock.calls[0];
    expect(options).toMatchObject({ title: "Update comic details?" });
    expect(text).toContain(
      "“Storm v02” is on your Kobo as the original CBZ file",
    );
    expect(text).toContain("You keep your place.");
    expect(text).toContain("remove it from your Kobo and send it again");
    // Cancelled: nothing is written and no comic preparation is offered.
    expect(ask).toHaveBeenCalledTimes(1);
    expect(invoke).not.toHaveBeenCalledWith("update_books", expect.anything());
    expect(invoke).not.toHaveBeenCalledWith(
      "comic_transfer_estimate",
      expect.anything(),
    );
  });
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

describe("library toolbar", () => {
  it("groups library tools and opens an action from the menu", async () => {
    await render();
    expect(
      panel("Library").querySelector('[aria-label="Library controls"]'),
    ).not.toBeNull();
    expect(panel("Library").querySelector('[role="menu"]')).toBeNull();
    const tools = button("Library tools ▾");
    await click(tools);
    expect(tools.getAttribute("aria-expanded")).toBe("true");
    expect(
      [...panel("Library").querySelectorAll('[role="menuitem"]')].map(
        (el) => el.textContent,
      ),
    ).toEqual([
      "Add comic folder…",
      "Find duplicates…",
      "Check library…",
      "Revert changes…",
      "Import from calibre…",
      "Tidy genres…",
      "Find metadata…",
    ]);
    await act(async () => {
      tools.dispatchEvent(
        new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
      );
    });
    expect(tools.getAttribute("aria-expanded")).toBe("false");
    expect(document.activeElement).toBe(tools);
    await click(tools);
    await click(
      [
        ...panel("Library").querySelectorAll<HTMLButtonElement>(
          '[role="menuitem"]',
        ),
      ].find((el) => el.textContent === "Revert changes…")!,
    );
    expect(panel("Library").querySelector('[role="menu"]')).toBeNull();
    // StrictMode runs the dialog's mount effect twice.
    expect(calls("library_snapshots")).toHaveLength(2);
  });
});

describe("permanent library details", () => {
  it("clears selection when a kind edit moves the book out of the filter", async () => {
    const comic: Book = {
      ...localBook,
      format: "cbz",
      kind: "manga",
      pages: 20,
    };
    const normal = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      switch (command) {
        case "library_list":
          return [comic];
        case "library_set_comic_kind":
          return { ...comic, kind: "comic" };
        case "library_comic_spreads":
          return 0;
        default:
          return normal(command, args);
      }
    });
    await render();
    const library = panel("Library");
    const change = async (select: HTMLSelectElement, value: string) => {
      await act(async () => {
        select.value = value;
        select.dispatchEvent(new Event("change", { bubbles: true }));
      });
    };
    await change(
      library.querySelector('[aria-label="Filter by kind"]')!,
      "manga",
    );
    await click(library.querySelector("button.card")!);
    await change(library.querySelector(".panel .form select")!, "comic");
    expect(calls("library_set_comic_kind")).toHaveLength(1);
    expect(library.querySelector("button.card")).toBeNull();
    expect(library.querySelector(".panel-empty")?.textContent).toContain(
      "Select a book",
    );
    expect(
      [...library.querySelectorAll("footer button")].some(
        (button) => button.textContent === "Remove",
      ),
    ).toBe(false);
  });

  it("keeps its slot through selection, switching, multiple selection and clearing", async () => {
    const normal = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (command, args) =>
      command === "library_list"
        ? [localBook, { ...localBook, id: 2, title: "Second book" }]
        : normal(command, args),
    );
    await render();
    const library = panel("Library");
    const slot = library.querySelector(".panel-slot")!;
    const cards = library.querySelectorAll<HTMLButtonElement>("button.card");
    const expectSlot = () => {
      expect(library.querySelector(".panel-slot")).toBe(slot);
      expect(slot.querySelector(".panel")).not.toBeNull();
      expect([...library.querySelectorAll("button.card")]).toEqual([...cards]);
    };
    expect(slot.textContent).toContain("Select a book");
    await click(cards[0]);
    expectSlot();
    expect(slot.querySelector("form")).not.toBeNull();
    await click(cards[1]);
    expectSlot();
    expect(slot.querySelector<HTMLInputElement>("input")?.value).toBe(
      "Second book",
    );
    await act(async () => {
      cards[0].dispatchEvent(
        new MouseEvent("click", { bubbles: true, metaKey: true }),
      );
    });
    expectSlot();
    expect(slot.textContent).toContain("2 books selected");
    await click(cards[0]);
    await click(slot.querySelector<HTMLButtonElement>('[aria-label="Close"]')!);
    expectSlot();
    expect(slot.textContent).toContain("Select a book");
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

  it("filters the Library by what's on the Kobo and drops hidden selections", async () => {
    const normal = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(async (command, args) =>
      command === "books_on_device"
        ? [
            {
              book_id: localBook.id,
              device_path: "book.epub",
              content_id: koboBook.id,
              sent_at: "2026-01-01T00:00:00Z",
              stale: false,
            },
          ]
        : normal(command, args),
    );
    await render();
    const cards = () => panel("Library").querySelectorAll("button.card");
    await click(cards()[0] as HTMLElement);
    expect(panel("Library").textContent).toContain("1 selected");

    await click(button("On Kobo 1"));
    expect(cards()).toHaveLength(1);
    expect(panel("Library").textContent).toContain("1 selected");

    await click(button("Not on Kobo 0"));
    expect(cards()).toHaveLength(0);
    expect(panel("Library").textContent).toContain("No books match these");
    expect(panel("Library").textContent).not.toContain("1 selected");

    await click(button("Clear filters"));
    expect(cards()).toHaveLength(1);
  });

  it("offers only library filters with no Kobo connected", async () => {
    connected = [];
    await render();
    const library = panel("Library");
    expect(
      library.querySelector('[aria-label="Filter by genre"]'),
    ).toBeTruthy();
    expect(library.querySelector('[aria-label="Filter by collection"]')).toBe(
      null,
    );
    expect(library.querySelector(".filter-bar .segmented")).toBe(null);
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
  it.each(["grid", "list"])(
    "requests Library and hidden Kobo thumbnails on mount in %s view",
    async (mode) => {
      localStorage.setItem("library-view", mode);
      localStorage.setItem("device-view", mode);
      await render();
      const selector = mode === "grid" ? ".card img" : ".list-row img";
      expect(panel("On Kobo").hidden).toBe(true);
      for (const name of ["Library", "On Kobo"]) {
        const image = panel(name).querySelector(selector);
        expect(image?.getAttribute("loading")).toBe("eager");
        expect(image?.getAttribute("decoding")).toBe("sync");
      }
    },
  );

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
  expect(calls("show_window")).toHaveLength(0);
  await act(async () => {
    finish([localBook]);
  });
  await act(async () => {
    await vi.advanceTimersByTimeAsync(150);
  });
  expect(calls("show_window")).toHaveLength(1);
  expect(panel("Library").textContent).toContain("Local book");
});
