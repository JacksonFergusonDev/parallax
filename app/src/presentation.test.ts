// @vitest-environment jsdom
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { prepareCovers, prepareWindow } from "./presentation";

let root: HTMLDivElement;
let stop: (() => void) | undefined;
beforeEach(() => {
  vi.useFakeTimers();
  root = document.createElement("div");
  document.body.append(root);
});
afterEach(() => {
  stop?.();
  stop = undefined;
  root.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

it("warms hidden covers in small batches and discovers new or changed covers", async () => {
  const decode = vi.fn(async () => {});
  Object.defineProperty(HTMLImageElement.prototype, "decode", {
    configurable: true,
    writable: true,
    value: decode,
  });
  root.innerHTML = '<div hidden><img src="a"><img src="b"><img src="c"></div>';
  stop = prepareCovers(root);
  await vi.advanceTimersByTimeAsync(32);
  expect(decode).toHaveBeenCalledTimes(2);
  expect(root.querySelector("img")!.loading).toBe("eager");
  await vi.advanceTimersByTimeAsync(32);
  expect(decode).toHaveBeenCalledTimes(3);
  root.querySelector("img")!.src = "new";
  root.append(document.createElement("img"));
  root.lastElementChild!.setAttribute("src", "d");
  await vi.advanceTimersByTimeAsync(100);
  expect(decode).toHaveBeenCalledTimes(5);
  stop();
  root.querySelector("img")!.src = "after-stop";
  await vi.advanceTimersByTimeAsync(100);
  expect(decode).toHaveBeenCalledTimes(5);
});

it("continues after broken and stalled covers without starting overlapping batches", async () => {
  const decode = vi
    .fn()
    .mockRejectedValueOnce(new Error("missing"))
    .mockImplementationOnce(() => new Promise(() => {}))
    .mockResolvedValue(undefined);
  Object.defineProperty(HTMLImageElement.prototype, "decode", {
    configurable: true,
    writable: true,
    value: decode,
  });
  root.innerHTML = '<img src="a"><img src="b"><img src="c">';
  stop = prepareCovers(root);
  await vi.advanceTimersByTimeAsync(32);
  root.append(document.createElement("span"));
  await vi.advanceTimersByTimeAsync(1000);
  expect(decode).toHaveBeenCalledTimes(2);
  await vi.advanceTimersByTimeAsync(1100);
  expect(decode).toHaveBeenCalledTimes(3);
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
