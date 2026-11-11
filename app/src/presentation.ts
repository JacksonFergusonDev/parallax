import { useEffect, type RefObject } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";

const gridCover = ".card > .cover > img[src]";
const coverDecodeBatchSize = 8;

/** Warm the decoded pixels of retained grid covers, including the hidden Kobo
 * grid. The image elements already request their thumbnails on mount. */
export function prepareGridCovers(root: HTMLElement): () => void {
  const pending = new Set<HTMLImageElement>();
  const prepared = new WeakMap<HTMLImageElement, string>();
  let running = false;

  function collect(node: Node) {
    if (!(node instanceof Element)) return;
    if (node instanceof HTMLImageElement && node.matches(gridCover)) {
      pending.add(node);
    } else {
      for (const image of node.querySelectorAll<HTMLImageElement>(gridCover)) {
        pending.add(image);
      }
    }
  }

  async function drain() {
    if (running) return;
    running = true;
    while (pending.size) {
      const batch: HTMLImageElement[] = [];
      for (const image of pending) {
        pending.delete(image);
        if (!root.contains(image) || prepared.get(image) === image.src)
          continue;
        prepared.set(image, image.src);
        batch.push(image);
        if (batch.length === coverDecodeBatchSize) break;
      }
      // Limit concurrent decodes and keep a stalled thumbnail from holding
      // up the remaining batches indefinitely.
      await Promise.allSettled(
        batch.map(async (image) => bounded(image.decode(), 2000)),
      );
    }
    running = false;
  }

  const observer = new MutationObserver((records) => {
    for (const record of records) {
      if (record.type === "attributes") collect(record.target);
      for (const node of record.addedNodes) collect(node);
    }
    void drain();
  });
  observer.observe(root, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["src"],
  });
  collect(root);
  void drain();
  return () => {
    observer.disconnect();
    pending.clear();
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
    if (root.current) return prepareGridCovers(root.current);
  }, [root]);

  useEffect(() => {
    if (!loaded || !root.current || !isTauri()) return;
    let cancelled = false;
    void prepareWindow(root.current).then(() => {
      if (!cancelled) {
        void invoke("show_window").catch(console.error);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [loaded, root]);
}
