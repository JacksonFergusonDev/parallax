import { useEffect, type RefObject } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";

/** Prepare the actual retained image elements, including the hidden Kobo tab.
 * Two decodes at a time, with a yield between batches, leave room for input.
 * Browsers can still evict decoded images under memory pressure. */
export function prepareCovers(root: HTMLElement): () => void {
  let stopped = false;
  let running = false;
  let timer: ReturnType<typeof setTimeout>;
  const prepared = new WeakMap<HTMLImageElement, string>();
  const run = async () => {
    if (stopped || running) return;
    const batch = Array.from(
      root.querySelectorAll<HTMLImageElement>("img[src]"),
    )
      .filter((image) => prepared.get(image) !== image.src)
      .slice(0, 2);
    if (!batch.length) return;
    running = true;
    await Promise.allSettled(
      batch.map(async (image) => {
        prepared.set(image, image.src);
        image.loading = "eager";
        image.decoding = "async";
        // A broken or stalled image must not prevent later covers warming.
        await bounded(image.decode(), 2000);
      }),
    );
    running = false;
    if (!stopped) timer = setTimeout(() => void run(), 32);
  };
  const schedule = () => {
    clearTimeout(timer);
    timer = setTimeout(() => void run(), 32);
  };
  const observer = new MutationObserver(schedule);
  observer.observe(root, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["src"],
  });
  schedule();
  return () => {
    stopped = true;
    clearTimeout(timer);
    observer.disconnect();
  };
}

async function bounded(work: Promise<unknown>, milliseconds: number) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    await Promise.race([
      work,
      new Promise((resolve) => {
        timer = setTimeout(resolve, milliseconds);
      }),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

/** Wait for the initial viewport, rather than every offscreen book or USB read. */
export async function prepareWindow(root: HTMLElement) {
  const images = Array.from(
    root.querySelectorAll<HTMLImageElement>("img"),
  ).filter((image) => {
    const rect = image.getBoundingClientRect();
    return rect.width > 0 && rect.top < innerHeight && rect.bottom > 0;
  });
  await bounded(
    Promise.allSettled([
      document.fonts.ready,
      ...images.map((image) => {
        image.loading = "eager";
        return image.decode();
      }),
    ]),
    2000,
  );
  // Allow React's committed layout and decoded covers to reach the compositor.
  // The timer also works when a hidden webview throttles animation frames.
  await bounded(
    new Promise((resolve) => {
      requestAnimationFrame(() => requestAnimationFrame(resolve));
    }),
    100,
  );
}

export function usePresentation(
  root: RefObject<HTMLElement | null>,
  loaded: boolean,
) {
  useEffect(() => {
    if (!root.current) return;
    return prepareCovers(root.current);
  }, [root]);

  useEffect(() => {
    if (!loaded || !root.current || !isTauri()) return;
    let cancelled = false;
    void prepareWindow(root.current).then(() => {
      if (!cancelled) {
        void invoke("show_main_window").catch(console.error);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [loaded, root]);
}
