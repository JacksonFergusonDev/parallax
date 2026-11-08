// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  menuState,
  resetMenu,
  runMenuCommand,
  sortCommands,
  useMenuCommands,
  type MenuCommands,
} from "./menu";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: () => false,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

function Commands({ commands }: { commands: MenuCommands }) {
  useMenuCommands(commands);
  return null;
}

describe("menu commands", () => {
  afterEach(() => resetMenu());

  it("registers while mounted, runs only what's enabled, and forgets on unmount", async () => {
    (
      globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    const send = vi.fn();
    const eject = vi.fn();
    const root = createRoot(document.createElement("div"));
    await act(async () =>
      root.render(
        <Commands
          commands={{
            "kobo.send": { run: send, enabled: false },
            "kobo.eject": { run: eject },
            "view.grid": { run: vi.fn(), checked: true },
          }}
        />,
      ),
    );
    expect(menuState()).toEqual({
      "kobo.send": { enabled: false },
      "kobo.eject": { enabled: true },
      "view.grid": { enabled: true, checked: true },
    });
    runMenuCommand("kobo.send");
    runMenuCommand("kobo.eject");
    runMenuCommand("nothing");
    expect(send).not.toHaveBeenCalled();
    expect(eject).toHaveBeenCalledOnce();

    // A command a later render leaves out is no longer offered.
    await act(async () =>
      root.render(<Commands commands={{ "kobo.eject": { run: eject } }} />),
    );
    expect(Object.keys(menuState())).toEqual(["kobo.eject"]);
    await act(async () => root.unmount());
    expect(menuState()).toEqual({});
  });

  it("sorts by a key, keeping the direction, and checks the current one", () => {
    const setSort = vi.fn();
    const commands = sortCommands({ key: "title", dir: "asc" }, setSort, [
      "title",
      "author",
    ] as const);
    expect(commands["sort.title"].checked).toBe(true);
    expect(commands["sort.asc"].checked).toBe(true);
    expect(commands["sort.default"].checked).toBe(false);
    commands["sort.author"].run();
    expect(setSort).toHaveBeenCalledWith({ key: "author", dir: "asc" });
    commands["sort.desc"].run();
    expect(setSort).toHaveBeenCalledWith({ key: "title", dir: "desc" });

    const unsorted = sortCommands(null, setSort, ["title"] as const);
    expect(unsorted["sort.default"].checked).toBe(true);
    expect(unsorted["sort.asc"].enabled).toBe(false);
  });
});

describe("menu ids", () => {
  it("names only items the menu bar has", async () => {
    // @ts-expect-error Node types are not part of the browser application.
    const { readFileSync } = await import("node:fs");
    const read = (path: string): string =>
      readFileSync(new URL(path, import.meta.url), "utf8");
    const menu = read("../src-tauri/src/menu.rs");
    const sorts = [...menu.matchAll(/\("(\w+)", "[^"]+"\)/g)].map(
      (m) => `sort.${m[1]}`,
    );
    const known = new Set([
      ...[...menu.matchAll(/command\("([\w.]+)"/g)].map((m) => m[1]),
      ...sorts,
    ]);
    const used = ["./App.tsx", "./components/DeviceView.tsx", "./menu.ts"]
      .flatMap((path) => [
        ...read(path).matchAll(
          /"((?:file|edit|view|library|kobo|sort)\.\w+)"/g,
        ),
      ])
      .map((m) => m[1]);
    expect(used.length).toBeGreaterThan(20);
    for (const id of used) expect(known, id).toContain(id);
    // Sort keys from both views are in Sort By.
    for (const key of ["title", "tags", "status", "last_read", "color"])
      expect(known).toContain(`sort.${key}`);
  });
});
