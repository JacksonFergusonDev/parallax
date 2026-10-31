import { describe, expect, it, vi } from "vitest";
import { deviceCoverSrc, type Book, type DeviceBook } from "./api";

vi.mock("@tauri-apps/api/core", () => ({
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
