import { useEffect, type RefObject } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";

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
