import { describe, expect, it, vi } from "vitest";
import { keyAction, percent, wheelPager, within } from "./paging";

const wheel = (deltaY: number, timeStamp: number, deltaX = 0) => ({
  deltaX,
  deltaY,
  ctrlKey: false,
  timeStamp,
});

describe("wheelPager", () => {
  it("turns once for a whole gesture", () => {
    const turn = vi.fn();
    const onWheel = wheelPager(turn);
    for (let t = 0; t < 500; t += 16) onWheel(wheel(30, t));
    expect(turn).toHaveBeenCalledTimes(1);
    expect(turn).toHaveBeenCalledWith(1);
  });

  it("turns again after a pause", () => {
    const turn = vi.fn();
    const onWheel = wheelPager(turn);
    onWheel(wheel(60, 0));
    onWheel(wheel(-60, 1000));
    expect(turn.mock.calls).toEqual([[1], [-1]]);
  });

  it("ignores small movements and pinches", () => {
    const turn = vi.fn();
    const onWheel = wheelPager(turn);
    onWheel(wheel(5, 0));
    onWheel({ ...wheel(100, 500), ctrlKey: true });
    expect(turn).not.toHaveBeenCalled();
  });

  it("uses whichever direction is stronger", () => {
    const turn = vi.fn();
    wheelPager(turn)(wheel(2, 0, -80));
    expect(turn).toHaveBeenCalledWith(-1);
  });
});

describe("keyAction", () => {
  const press = (key: string, extra = {}) => ({
    key,
    shiftKey: false,
    metaKey: false,
    ctrlKey: false,
    altKey: false,
    ...extra,
  });

  it("maps page keys", () => {
    expect(keyAction(press("ArrowLeft"))).toBe("left");
    expect(keyAction(press(" "))).toBe("next");
    expect(keyAction(press(" ", { shiftKey: true }))).toBe("previous");
    expect(keyAction(press("Escape"))).toBe("close");
  });

  it("finds with ⌘F and ⌘G", () => {
    expect(keyAction(press("f", { metaKey: true }))).toBe("find");
    expect(keyAction(press("g", { metaKey: true }))).toBe("findNext");
    expect(keyAction(press("g", { metaKey: true, shiftKey: true }))).toBe(
      "findPrevious",
    );
  });

  it("lets typing in a field through", () => {
    expect(keyAction(press(" ", { typing: true }))).toBeNull();
    expect(keyAction(press("ArrowRight", { typing: true }))).toBeNull();
    expect(keyAction(press("Escape", { typing: true }))).toBe("close");
    expect(keyAction(press("g", { typing: true, metaKey: true }))).toBe(
      "findNext",
    );
  });

  it("leaves system shortcuts alone", () => {
    expect(keyAction(press("w", { metaKey: true }))).toBeNull();
    expect(keyAction(press("ArrowLeft", { metaKey: true }))).toBeNull();
    expect(keyAction(press("=", { metaKey: true }))).toBe("larger");
  });
});

describe("percent", () => {
  it("never goes past the ends", () => {
    expect(percent(2)).toBe("100%");
    expect(percent(-1)).toBe("0%");
    expect(percent(0.255)).toBe("26%");
  });
});

describe("within", () => {
  it("rejects work that never finishes", async () => {
    await expect(within(new Promise(() => {}), 1)).rejects.toThrow("timed out");
  });

  it("passes results through", async () => {
    await expect(within(Promise.resolve(3), 50)).resolves.toBe(3);
  });
});
