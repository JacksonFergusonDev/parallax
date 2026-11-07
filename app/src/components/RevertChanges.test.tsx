// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  describeEntries,
  describeSnapshot,
  RevertChanges,
} from "./RevertChanges";
import type { DeviceInfo, LogEntry, Snapshot } from "../api";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string) => `asset:${path}`,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn() }));

const device = { mount: "/Volumes/K", model: "Kobo Libra 2" } as DeviceInfo;
const entry = (op: string, ids: string[], error?: string): LogEntry => ({
  at: "2026-11-05T12:00:00Z",
  erc_version: "0.1.0",
  op,
  content_ids: ids,
  error,
});
const backups = [
  {
    file: "KoboReader-b.sqlite",
    at: "2026-11-05T12:00:00Z",
    entries: [entry("remove", ["file:///mnt/onboard/KEpub/Dune.kepub.epub"])],
    removed_files: 1,
    db_version: 174,
    blocked: null,
  },
  {
    file: "KoboReader-a.sqlite",
    at: "2026-09-01T12:00:00Z",
    entries: [],
    removed_files: 0,
    db_version: 170,
    blocked: "this backup is from database version 170",
  },
];
const snapshots = [
  {
    file: "library-a.sqlite",
    at: "2026-11-05T12:00:00Z",
    op: "tidy",
    detail: null,
    books: 3,
  },
];

describe("describing changes", () => {
  it("names one book and counts several", () => {
    const one = "file:///mnt/onboard/KEpub/Dune - Frank Herbert.kepub.epub";
    expect(describeEntries([entry("remove", [one])])).toBe(
      "Removed Dune - Frank Herbert",
    );
    expect(
      describeEntries([entry("remove", ["a"]), entry("remove", ["b"])]),
    ).toBe("Removed 2 books");
  });

  it("copes with no record and marks failures", () => {
    expect(describeEntries([])).toBe("An earlier change");
    expect(describeEntries([entry("remove", ["a"], "busy")])).toContain(
      "(failed)",
    );
    expect(describeEntries([entry("restore", [])])).toBe("Reverting changes");
  });

  it("describes library snapshots", () => {
    const s = { op: "remove", detail: "3 books" } as Snapshot;
    expect(describeSnapshot(s)).toBe("Removed 3 books");
  });
});

describe("RevertChanges", () => {
  let root: Root;
  let host: HTMLDivElement;
  const onReverted = vi.fn();
  const onClose = vi.fn();

  const button = (text: string) =>
    [...host.querySelectorAll("button")].find((b) =>
      b.textContent?.includes(text),
    ) as HTMLButtonElement;
  const click = (b: HTMLElement) =>
    act(async () => {
      b.click();
      await Promise.resolve();
    });
  const open = (d: DeviceInfo | null, start: "library" | "kobo") =>
    act(async () => {
      root.render(
        <RevertChanges
          device={d}
          start={start}
          onReverted={onReverted}
          onClose={onClose}
        />,
      );
      await Promise.resolve();
    });

  beforeEach(() => {
    (
      globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    host = document.body.appendChild(document.createElement("div"));
    root = createRoot(host);
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "device_backups") return backups;
      if (command === "library_snapshots") return snapshots;
      if (command === "device_restore")
        return { safety_backup: "s", files_restored: 1 };
      if (command === "library_restore")
        return { safety_snapshot: "s", restored: 2, set_aside: 1, lost: [] };
      throw new Error(`unexpected ${command}`);
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.clearAllMocks();
  });

  it("shows a Kobo's backups and won't offer a blocked one", async () => {
    await open(device, "kobo");
    expect(host.textContent).toContain("Before: Removed Dune");
    expect(host.textContent).toContain("1 removed book can come back");
    expect(button("An earlier change").disabled).toBe(true);
    expect(button("Revert…").disabled).toBe(true);
  });

  it("restores only after confirmation, with what it warned about", async () => {
    await open(device, "kobo");
    await click(button("Removed Dune"));

    vi.mocked(ask).mockResolvedValueOnce(false);
    await click(button("Revert…"));
    expect(invoke).not.toHaveBeenCalledWith(
      "device_restore",
      expect.anything(),
    );
    expect(vi.mocked(ask).mock.calls[0][0]).toContain("Reading progress");

    vi.mocked(ask).mockResolvedValueOnce(true);
    await click(button("Revert…"));
    expect(invoke).toHaveBeenCalledWith("device_restore", {
      mount: "/Volumes/K",
      file: "KoboReader-b.sqlite",
    });
    expect(onReverted).toHaveBeenCalledWith(
      "kobo",
      expect.stringContaining("1 book put back"),
    );
  });

  it("offers the library alone with no Kobo connected", async () => {
    await open(null, "kobo");
    expect(button("Kobo Libra 2")).toBeUndefined();
    await click(button("Tidied genres"));
    vi.mocked(ask).mockResolvedValueOnce(true);
    await click(button("Revert…"));
    expect(onReverted).toHaveBeenCalledWith(
      "library",
      "Library reverted. 2 books brought back. 1 book added since set aside.",
    );
  });

  it("shows why going back failed and stays open", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "library_snapshots") return snapshots;
      throw new Error("the snapshot is damaged");
    });
    await open(null, "library");
    await click(button("Tidied genres"));
    vi.mocked(ask).mockResolvedValueOnce(true);
    await click(button("Revert…"));
    expect(host.textContent).toContain("the snapshot is damaged");
    expect(onReverted).not.toHaveBeenCalled();
    expect(button("Revert…").disabled).toBe(false);
  });
});
