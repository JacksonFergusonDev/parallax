import { describe, expect, it } from "vitest";
import { openingView } from "./preferences";

describe("opening view", () => {
  it("opens on the library unless Settings say otherwise", () => {
    expect(openingView(null, "reading")).toBeNull();
    expect(openingView("reading", null)).toBe("reading");
    expect(openingView("last", "device")).toBe("device");
    expect(openingView("last", null)).toBeNull();
    expect(openingView("last", "elsewhere")).toBeNull();
  });
});
