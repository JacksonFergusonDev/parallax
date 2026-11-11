// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { prepareWindow } from "./presentation";

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
