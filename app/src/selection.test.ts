import { describe, expect, it } from "vitest";
import { clickSelect } from "./selection";

const plain = { shiftKey: false, metaKey: false, ctrlKey: false };
const cmd = { ...plain, metaKey: true };
const shift = { ...plain, shiftKey: true };
const order = ["a", "b", "c", "d"];

describe("clickSelect", () => {
  it("selects one item per plain click and clears on a second click", () => {
    const anchor = { current: null as string | null };
    let s = clickSelect(new Set<string>(), "a", order, anchor, plain);
    expect([...s]).toEqual(["a"]);
    s = clickSelect(s, "b", order, anchor, plain);
    expect([...s]).toEqual(["b"]);
    s = clickSelect(s, "b", order, anchor, plain);
    expect([...s]).toEqual([]);
  });

  it("adds and removes with ⌘ and extends ranges with Shift", () => {
    const anchor = { current: null as string | null };
    let s = clickSelect(new Set<string>(), "a", order, anchor, plain);
    s = clickSelect(s, "c", order, anchor, cmd);
    expect([...s].sort()).toEqual(["a", "c"]);
    s = clickSelect(s, "a", order, anchor, cmd);
    expect([...s]).toEqual(["c"]);
    s = clickSelect(s, "c", order, anchor, cmd);
    expect([...s]).toEqual([]);
    s = clickSelect(s, "b", order, anchor, cmd);
    s = clickSelect(s, "d", order, anchor, shift);
    expect([...s].sort()).toEqual(["b", "c", "d"]);
    s = clickSelect(s, "c", order, anchor, { ...plain, ctrlKey: true });
    expect([...s].sort()).toEqual(["b", "d"]);
  });
});
