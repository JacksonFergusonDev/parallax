// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { prepareGridCovers, prepareWindow } from "./presentation";

let root: HTMLDivElement;
beforeEach(() => {
  vi.useFakeTimers();
  root = document.createElement("div");
  document.body.append(root);
});
afterEach(() => {
  root.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

it("decodes hidden grid covers on mount and warms new and changed sources", async () => {
  root.innerHTML = `<div hidden>
    <div class="card"><div class="cover"><img src="first"></div></div>
  </div><img src="detail">`;
  const [cover, detail] = root.querySelectorAll("img");
  cover.decode = vi.fn(async () => {});
  detail.decode = vi.fn(async () => {});
  const stop = prepareGridCovers(root);
  try {
    expect(cover.decode).toHaveBeenCalledOnce();
    expect(detail.decode).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(0);

    const newCard = root.querySelector(".card")!.cloneNode(true) as HTMLElement;
    const newCover = newCard.querySelector("img")!;
    newCover.decode = vi.fn(async () => {});
    root.append(newCard);
    cover.src = "replacement";
    await vi.advanceTimersByTimeAsync(0);
    expect(cover.decode).toHaveBeenCalledTimes(2);
    expect(newCover.decode).toHaveBeenCalledOnce();
  } finally {
    stop();
  }
  cover.src = "after-stop";
  await vi.advanceTimersByTimeAsync(0);
  expect(cover.decode).toHaveBeenCalledTimes(2);
});

it("limits concurrent decodes and continues past broken and stalled covers", async () => {
  root.innerHTML =
    '<div class="card"><div class="cover"><img src="cover"></div></div>'.repeat(
      9,
    );
  const covers = [...root.querySelectorAll("img")];
  const decode = vi.fn(async () => {});
  for (const cover of covers) cover.decode = decode;
  covers[0].decode = vi.fn(() => new Promise<void>(() => {}));
  covers[1].decode = vi.fn(async () => {
    throw new Error("unreadable");
  });
  const stop = prepareGridCovers(root);
  try {
    await vi.advanceTimersByTimeAsync(1999);
    expect(decode).toHaveBeenCalledTimes(6);
    await vi.advanceTimersByTimeAsync(1);
    expect(decode).toHaveBeenCalledTimes(7);
  } finally {
    stop();
  }
});

it("waits for initial visible covers and fonts, with a bound for stalled assets and hidden frames", async () => {
  root.innerHTML = '<img src="visible"><img src="hidden">';
  const [visible, hidden] = root.querySelectorAll("img");
  vi.spyOn(visible, "getBoundingClientRect").mockReturnValue({
    width: 100,
    top: 10,
    bottom: 160,
  } as DOMRect);
  visible.decode = vi.fn(() => new Promise<void>(() => {}));
  hidden.decode = vi.fn();
  Object.defineProperty(document, "fonts", {
    configurable: true,
    value: { ready: Promise.resolve() },
  });
  vi.stubGlobal("requestAnimationFrame", vi.fn());
  let ready = false;
  const work = prepareWindow(root).then(() => {
    ready = true;
  });
  await vi.advanceTimersByTimeAsync(1999);
  expect(ready).toBe(false);
  expect(visible.decode).toHaveBeenCalledOnce();
  expect(hidden.decode).not.toHaveBeenCalled();
  await vi.advanceTimersByTimeAsync(101);
  await work;
  expect(ready).toBe(true);
});
