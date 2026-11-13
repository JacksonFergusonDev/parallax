import { afterEach, describe, expect, it, vi } from "vitest";
import { ReadCache } from "./readCache";

afterEach(() => vi.useRealTimers());

describe("read cache", () => {
  it("shares pending requests and reuses successful results", async () => {
    const cache = new ReadCache();
    const load = vi.fn(async () => ["book"]);
    const first = cache.get(["books"], load, ["library"]);
    expect(cache.get(["books"], load, ["library"])).toBe(first);
    await first;
    expect(await cache.get(["books"], load, ["library"])).toEqual(["book"]);
    expect(load).toHaveBeenCalledTimes(1);
  });

  it("expires results and retries failures", async () => {
    vi.useFakeTimers();
    const cache = new ReadCache();
    const load = vi.fn(async () => "books");
    await cache.get(["books"], load, [], 100);
    vi.advanceTimersByTime(101);
    await cache.get(["books"], load, [], 100);
    expect(load).toHaveBeenCalledTimes(2);
    const failing = vi
      .fn()
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValue("online");
    await expect(cache.get(["retry"], failing, [])).rejects.toThrow("offline");
    expect(await cache.get(["retry"], failing, [])).toBe("online");
  });

  it("invalidates related data without discarding other devices", async () => {
    const cache = new ReadCache();
    const load = vi.fn(async () => "data");
    await cache.get(["books", "a"], load, ["device:a"]);
    await cache.get(["books", "b"], load, ["device:b"]);
    cache.invalidate("device:a");
    await cache.get(["books", "b"], load, ["device:b"]);
    expect(load).toHaveBeenCalledTimes(2);
    await cache.get(["books", "a"], load, ["device:a"]);
    expect(load).toHaveBeenCalledTimes(3);
  });

  it("does not let an invalidated pending read replace newer data", async () => {
    const cache = new ReadCache();
    let finish!: (value: string) => void;
    const old = cache.get(
      ["books"],
      () =>
        new Promise<string>((resolve) => {
          finish = resolve;
        }),
      ["device"],
    );
    await Promise.resolve();
    cache.invalidate("device");
    await cache.get(["books"], async () => "new", ["device"]);
    finish("old");
    await old;
    expect(
      await cache.get(["books"], async () => "unexpected", ["device"]),
    ).toBe("new");
  });

  it("evicts the least recently used entry to bound memory", async () => {
    const cache = new ReadCache(2);
    const load = vi.fn(async () => "data");
    await cache.get(["a"], load, []);
    await cache.get(["b"], load, []);
    await cache.get(["a"], load, []);
    await cache.get(["c"], load, []);
    await cache.get(["a"], load, []);
    expect(load).toHaveBeenCalledTimes(3);
    await cache.get(["b"], load, []);
    expect(load).toHaveBeenCalledTimes(4);
  });
});

describe("cover dimension caching", () => {
  it("reuses image measurements without downloading full-size covers again", async () => {
    const { coverDimensions, readCache } = await import("./readCache");
    readCache.clear();
    let downloads = 0;
    class FakeImage {
      naturalWidth = 1200;
      naturalHeight = 1800;
      onload = () => {};
      set src(_url: string) {
        downloads++;
        queueMicrotask(() => this.onload());
      }
    }
    vi.stubGlobal("Image", FakeImage);
    try {
      const first = coverDimensions("https://example.test/cover.jpg");
      const second = coverDimensions("https://example.test/cover.jpg");
      expect(first).toBe(second);
      expect(await first).toEqual([1200, 1800]);
      expect(await coverDimensions("https://example.test/cover.jpg")).toEqual([
        1200, 1800,
      ]);
      expect(downloads).toBe(1);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("peeks only at finished, unexpired reads", async () => {
    vi.useFakeTimers();
    const cache = new ReadCache();
    let finish!: (v: string) => void;
    const pending = cache.get(
      ["stats"],
      () => new Promise<string>((r) => (finish = r)),
      ["reading"],
      100,
    );
    expect(cache.peek(["stats"])).toBeUndefined();
    await vi.advanceTimersByTimeAsync(0);
    finish("loaded");
    await pending;
    expect(cache.peek(["stats"])).toBe("loaded");
    vi.advanceTimersByTime(101);
    expect(cache.peek(["stats"])).toBeUndefined();
  });

  it("stores a value read elsewhere and forgets it with its tags", async () => {
    const cache = new ReadCache();
    const load = vi.fn(async () => "stale");
    cache.put(["stats"], "fresh", ["reading"]);
    expect(cache.peek(["stats"])).toBe("fresh");
    expect(await cache.get(["stats"], load, ["reading"])).toBe("fresh");
    expect(load).not.toHaveBeenCalled();
    cache.invalidate("reading");
    expect(cache.peek(["stats"])).toBeUndefined();
  });
});
