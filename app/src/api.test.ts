import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  deviceCoverSrc,
  type Book,
  type DeviceBook,
  type DeviceInfo,
} from "./api";
import * as api from "./api";
import { invoke } from "@tauri-apps/api/core";
import { readCache } from "./readCache";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string) => `asset:${path}`,
}));

describe("device cover URLs", () => {
  const device = { cover: "/cache/kobo-cover.jpg" } as DeviceBook;
  const library = {
    cover: "/library/cover.jpg",
    modified_at: "2026-10-31",
  } as Book;

  it("uses the Kobo thumbnail even without a library copy", () => {
    expect(deviceCoverSrc(device)).toBe("asset:/cache/kobo-cover.jpg");
    expect(deviceCoverSrc(device, library)).toBe("asset:/cache/kobo-cover.jpg");
  });

  it("falls back to the library and handles books with no available cover", () => {
    expect(deviceCoverSrc({ cover: null } as DeviceBook, library)).toBe(
      "asset:/library/cover.jpg?v=2026-10-31",
    );
    expect(deviceCoverSrc({} as DeviceBook)).toBeNull();
  });
});

describe("API read caching", () => {
  beforeEach(() => {
    readCache.clear();
    vi.mocked(invoke).mockReset().mockResolvedValue([]);
  });

  it("deduplicates library reads and invalidates metadata and cover searches after edits", async () => {
    await Promise.all([api.listBooks(), api.listBooks()]);
    await api.searchCovers(1);
    await api.updateBook(1, {} as api.MetadataEdit);
    await api.listBooks();
    await api.searchCovers(1);
    expect(vi.mocked(invoke).mock.calls.map(([command]) => command)).toEqual([
      "library_list",
      "cover_search",
      "library_update",
      "library_list",
      "cover_search",
    ]);
  });

  it.each([
    ["send", () => api.sendBooks("a", [1])],
    ["update", () => api.updateBooks("a", [1])],
    ["remove", () => api.removeFromDevice("a", ["id"])],
    ["collections", () => api.changeCollections("a", [])],
    ["rules", () => api.rulesApply("a")],
    ["tidy", () => api.tidyDevice("a")],
    ["eject", () => api.ejectDevice("a")],
  ])("invalidates all related device reads after %s", async (_name, mutate) => {
    await api.deviceBooks("a");
    await api.deviceBooks("b");
    await api.listCollections("a");
    await api.deviceHistory("a");
    await mutate();
    await api.deviceBooks("a");
    await api.deviceBooks("b");
    await api.listCollections("a");
    await api.deviceHistory("a");
    const calls = vi.mocked(invoke).mock.calls;
    expect(
      calls.filter(
        ([command, args]) =>
          command === "device_books" &&
          (args as Record<string, unknown> | undefined)?.mount === "a",
      ),
    ).toHaveLength(2);
    expect(
      calls.filter(
        ([command, args]) =>
          command === "device_books" &&
          (args as Record<string, unknown> | undefined)?.mount === "b",
      ),
    ).toHaveLength(1);
    expect(
      calls.filter(([command]) => command === "list_collections"),
    ).toHaveLength(2);
    expect(
      calls.filter(([command]) => command === "device_history"),
    ).toHaveLength(2);
  });

  it("invalidates even when a write fails partway", async () => {
    await api.deviceBooks("a");
    vi.mocked(invoke).mockRejectedValueOnce(new Error("partial failure"));
    await expect(api.sendBooks("a", [1])).rejects.toThrow("partial failure");
    await api.deviceBooks("a");
    expect(
      vi
        .mocked(invoke)
        .mock.calls.filter(([command]) => command === "device_books"),
    ).toHaveLength(2);
  });

  it("refreshes device reads after a disconnect or a different serial on the same mount", async () => {
    let connected = [{ mount: "a", serial: "first" }] as DeviceInfo[];
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === "list_devices" ? connected : [],
    );
    await api.listDevices();
    await api.deviceBooks("a");
    await api.listDevices();
    await api.deviceBooks("a");
    expect(
      vi
        .mocked(invoke)
        .mock.calls.filter(([command]) => command === "device_books"),
    ).toHaveLength(1);
    connected = [];
    await api.listDevices();
    connected = [{ mount: "a", serial: "first" }] as DeviceInfo[];
    await api.listDevices();
    await api.deviceBooks("a");
    connected = [{ mount: "a", serial: "second" }] as DeviceInfo[];
    await api.listDevices();
    await api.deviceBooks("a");
    expect(
      vi
        .mocked(invoke)
        .mock.calls.filter(([command]) => command === "device_books"),
    ).toHaveLength(3);
  });

  it("discards device data when detection fails", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([{ mount: "a", serial: "first" }]);
    await api.listDevices();
    await api.deviceBooks("a");
    vi.mocked(invoke).mockRejectedValueOnce(new Error("detection failed"));
    await expect(api.listDevices()).rejects.toThrow("detection failed");
    await api.deviceBooks("a");
    expect(
      vi
        .mocked(invoke)
        .mock.calls.filter(([command]) => command === "device_books"),
    ).toHaveLength(2);
  });

  it("invalidates rule previews after library and rule changes", async () => {
    await api.rulesList();
    await api.rulesPreview("a");
    await api.rulesSave({} as api.Rule);
    await api.rulesList();
    await api.rulesPreview("a");
    await api.importBooks(["book.epub"]);
    await api.rulesPreview("a");
    const calls = vi.mocked(invoke).mock.calls;
    expect(calls.filter(([command]) => command === "rules_list")).toHaveLength(
      2,
    );
    expect(
      calls.filter(([command]) => command === "rules_preview"),
    ).toHaveLength(3);
  });
});
