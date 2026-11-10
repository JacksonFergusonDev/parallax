// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { DeviceInfo } from "../api";
import { DEFAULT_SETTINGS } from "../reader/backend";
import { SetupFlow } from "./SetupFlow";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: () => false,
  convertFileSrc: (path: string) => path,
  Channel: class {
    onmessage: (event: unknown) => void = () => {};
  },
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn() }));

let mounts = 0;
const kobo = (read_only: string | null = null): DeviceInfo => ({
  kind: "kobo",
  model: "Kobo Libra 2",
  firmware: "4.38.23697",
  serial: "N1",
  // A fresh mount each time keeps cached reads from crossing tests.
  mount: `/Volumes/KOBO${++mounts}`,
  capabilities: {
    native_collections: true,
    series: true,
    reading_progress: true,
    reading_time: true,
  },
  screen: { width: 1264, height: 1680, colour: false, source: "model" },
  read_only,
});

const deviceBook = (id: string) => ({
  id,
  title: id,
  author: null,
  path: null,
  sideloaded: true,
  series: null,
  series_number: null,
  read_status: "unread",
  percent_read: 0,
  last_read: null,
  reading_time_secs: null,
  added: null,
  file_size: 1,
  collections: [],
});

describe("first-run setup", () => {
  let root: Root;
  let host: HTMLDivElement;
  let stored: Record<string, string | null>;
  let props: Parameters<typeof SetupFlow>[0];

  const calls = (command: string) =>
    vi.mocked(invoke).mock.calls.filter(([c]) => c === command);
  const button = (text: string) =>
    [...host.querySelectorAll("button")].find(
      (b) => b.textContent?.trim() === text,
    ) as HTMLButtonElement | undefined;
  const title = () => host.querySelector("h1")?.textContent;
  const settle = () =>
    act(async () => {
      for (let i = 0; i < 8; i++) await Promise.resolve();
    });
  const click = async (text: string) => {
    const target = button(text);
    if (!target) throw new Error(`no ${text} button in “${title()}”`);
    await act(async () => target.click());
    await settle();
  };
  const render = async (changes: Partial<typeof props> = {}) => {
    props = { ...props, ...changes };
    await act(async () => root.render(<SetupFlow {...props} />));
    await settle();
  };

  beforeEach(async () => {
    (
      globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    window.matchMedia = vi.fn(() => ({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })) as unknown as typeof window.matchMedia;
    stored = {};
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      switch (command) {
        case "settings_all":
          return { ...stored };
        case "reader_settings_get":
          return { ...DEFAULT_SETTINGS };
        case "setting_set":
          stored[a.key as string] = a.value as string | null;
          return null;
        case "device_books":
          return ["a", "b", "c"].map(deviceBook);
        case "import_from_device":
          return [
            {
              content_id: "a",
              title: "A",
              result: { Ok: { kind: "added", book_id: 1 } },
            },
            {
              content_id: "b",
              title: "B",
              result: { Ok: { kind: "added", book_id: 2 } },
            },
          ];
        case "kobo_health":
          return {
            missing_files: [],
            not_imported: [],
            duplicate_collections: [],
            orphaned_images: 0,
            orphaned_image_bytes: 0,
          };
        default:
          throw new Error(`unexpected ${command}`);
      }
    });
    props = {
      device: null,
      libraryBooks: 0,
      onKobo: 0,
      adding: false,
      onLibraryChanged: vi.fn(async () => {}),
      onChooseFiles: vi.fn(),
      onChooseComicFolder: vi.fn(),
      onCalibre: vi.fn(),
      onFinish: vi.fn(),
    };
    host = document.body.appendChild(document.createElement("div"));
    root = createRoot(host);
    await render();
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("skips in one click from any step but the last", async () => {
    expect(title()).toBe("Welcome to Parallax");
    await click("Get Started");
    await click("Skip Setup");
    expect(props.onFinish).toHaveBeenCalledWith("skipped");
  });

  it("waits for a Kobo, offering to go on without one", async () => {
    await click("Get Started");
    expect(title()).toBe("Connect your Kobo");
    expect(button("Continue")).toBeUndefined();
    expect(host.textContent).toContain("Waiting for your Kobo");

    await render({ device: kobo() });
    expect(title()).toBe("Your Kobo is connected");
    expect(host.textContent).toContain("Kobo Libra 2");
    expect(host.textContent).toContain("3 books");
    expect(button("Continue")).toBeDefined();
  });

  it("goes on without a Kobo to the other ways of adding books", async () => {
    await click("Get Started");
    await click("Continue Without a Kobo");
    expect(title()).toBe("Add your books");
    await click("Import from calibre…");
    expect(props.onCalibre).toHaveBeenCalled();
    await render({ libraryBooks: 12 });
    expect(host.textContent).toContain("12 books in your library");
  });

  it("adds the Kobo's books and remembers it did", async () => {
    await render({ device: kobo(), onKobo: 1 });
    await click("Get Started");
    await click("Continue");
    expect(title()).toBe("Bring your Kobo's books home");
    expect(host.textContent).toContain("3 books, 1 already in your library");

    await click("Add 2 Books to Library");
    expect(calls("import_from_device")[0][1]).toMatchObject({
      contentIds: null,
    });
    expect(props.onLibraryChanged).toHaveBeenCalled();
    expect(host.textContent).toContain("Added 2 books to your library");

    // Going back and forth keeps the result rather than offering it again.
    await click("Back");
    await click("Continue");
    expect(button("Add 2 Books to Library")).toBeUndefined();
    expect(host.textContent).toContain("Added 2 books to your library");
  });

  const input = (text: string) =>
    [...host.querySelectorAll("label")]
      .find((l) => l.textContent?.includes(text))!
      .querySelector("input") as HTMLInputElement;
  const toggle = async (text: string) => {
    await act(async () => input(text).click());
    await settle();
  };

  it("checks the Kobo once it's connected", async () => {
    await render({ device: kobo() });
    await click("Get Started");
    expect(host.textContent).toContain("everything looks good");
  });

  it("recommends settings, with every one open to customise", async () => {
    await render({ device: kobo() });
    await click("Get Started");
    await click("Continue");
    await click("Continue");
    expect(title()).toBe("Choose your settings");
    expect(input("Recommended settings").checked).toBe(true);
    expect(host.textContent).not.toContain("Eject the Kobo");

    await toggle("Customise");
    expect(input("Eject the Kobo").checked).toBe(false);
    await toggle("Eject the Kobo");
    await toggle("Review books when they're added");
    await toggle("Read in one window");
    expect(stored).toMatchObject({
      eject_after_send: "on",
      review_on_add: "off",
      reader_windows: "one",
    });

    // Leaving and coming back keeps the choices open.
    await click("Continue");
    expect(title()).toBe("Choose a look");
    await click("Sepia");
    expect(stored.theme).toBe("sepia");
    await click("Back");
    expect(input("Customise").checked).toBe(true);

    // Going back to the recommendation undoes what was changed, not the look.
    await toggle("Recommended settings");
    expect(stored).toMatchObject({
      eject_after_send: null,
      review_on_add: null,
      reader_windows: null,
      theme: "sepia",
    });
    expect(host.textContent).not.toContain("Eject the Kobo");
  });

  it("opens on Customise when settings were already changed", async () => {
    stored.open_view = "last";
    await act(async () => root.unmount());
    root = createRoot(host);
    await render();
    await click("Get Started");
    await click("Continue Without a Kobo");
    await click("Continue");
    expect(input("Customise").checked).toBe(true);
    const open = [...host.querySelectorAll("select")].find((s) =>
      s.labels?.[0]?.textContent?.includes("Open Parallax to"),
    )!;
    expect(open.value).toBe("last");
  });

  it("doesn't check a Kobo it won't change", async () => {
    await render({ device: kobo("an untested model") });
    await click("Get Started");
    expect(host.textContent).toContain("won't change anything on it");
    await click("Continue");
    await click("Continue");
    expect(calls("kobo_health")).toHaveLength(0);
  });

  it("continues on Return, but never skips on Escape", async () => {
    const card = host.querySelector(".setup")!;
    const key = (k: string) =>
      act(async () => {
        host
          .querySelector("h1")!
          .dispatchEvent(
            new KeyboardEvent("keydown", { key: k, bubbles: true }),
          );
      });
    await key("Escape");
    expect(props.onFinish).not.toHaveBeenCalled();
    await key("Enter");
    await settle();
    expect(title()).toBe("Connect your Kobo");
    // Nothing to continue to while it waits for a Kobo.
    await key("Enter");
    expect(title()).toBe("Connect your Kobo");
    expect(card).toBeTruthy();
  });

  it("finishes with a summary and opens the library", async () => {
    await render({ device: kobo(), libraryBooks: 5 });
    await click("Get Started");
    for (let i = 0; i < 5; i++) await click("Continue");
    expect(title()).toBe("You're all set");
    expect(host.textContent).toContain("Kobo Libra 2 set up");
    expect(host.textContent).toContain("Recommended settings");
    expect(host.textContent).toContain("5 books in your library");
    expect(button("Skip Setup")).toBeUndefined();
    await click("Open Library");
    expect(props.onFinish).toHaveBeenCalledWith("done");
  });
});
