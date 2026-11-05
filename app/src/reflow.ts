// Smooth reflow for the cover grid. When the edit panel opens or closes the
// grid gains or loses columns and every card changes slot; instead of jumping,
// cards on screen glide from their old slot to the new one (FLIP).

import { useLayoutEffect, useRef, type RefObject } from "react";

/** Offset from a card's new slot back to its old one, in pixels. */
export function slotDelta(
  index: number,
  oldColumns: number,
  newColumns: number,
  colStride: number,
  rowStride: number,
): { dx: number; dy: number } {
  return {
    dx: ((index % oldColumns) - (index % newColumns)) * colStride,
    dy:
      (Math.floor(index / oldColumns) - Math.floor(index / newColumns)) *
      rowStride,
  };
}

function columnCount(grid: HTMLElement): number {
  const tracks = getComputedStyle(grid).gridTemplateColumns.trim();
  return tracks && tracks !== "none" ? tracks.split(/\s+/).length : 1;
}

const reducedMotion = () =>
  window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

/**
 * Animates the cards in `gridRef` to their new slots whenever `trigger`
 * changes and that changed the number of columns. Window resizes only update
 * the remembered column count, so dragging the window edge doesn't animate.
 */
export function useGridReflow(
  gridRef: RefObject<HTMLElement | null>,
  trigger: unknown,
) {
  const columns = useRef(0);
  const observed = useRef<{ el: HTMLElement; ro: ResizeObserver } | null>(null);

  // Remember the column count as the grid resizes for any reason. The grid
  // element comes and goes (list view, empty library), so check every commit.
  useLayoutEffect(() => {
    const el = gridRef.current;
    if (observed.current?.el === el) return;
    observed.current?.ro.disconnect();
    observed.current = null;
    if (!el || typeof ResizeObserver === "undefined") return;
    columns.current = columnCount(el);
    const ro = new ResizeObserver(() => {
      columns.current = columnCount(el);
    });
    ro.observe(el);
    observed.current = { el, ro };
  });
  useLayoutEffect(
    () => () => {
      observed.current?.ro.disconnect();
      observed.current = null;
    },
    [],
  );

  useLayoutEffect(() => {
    const el = gridRef.current;
    if (!el) return;
    const before = columns.current;
    const after = columnCount(el);
    columns.current = after;
    if (!before || before === after || reducedMotion()) return;

    const cards = el.children;
    if (cards.length < 2) return;
    const style = getComputedStyle(el);
    const first = (cards[0] as HTMLElement).getBoundingClientRect();
    const colStride = first.width + (parseFloat(style.columnGap) || 0);
    const rowStride = first.height + (parseFloat(style.rowGap) || 0);

    // Only cards on screen, before or after, are worth animating.
    const viewport = el.closest(".scroll") ?? document.documentElement;
    const view = viewport.getBoundingClientRect();
    const gridTop = el.getBoundingClientRect().top;
    const onScreen = (row: number) => {
      const top = gridTop + row * rowStride;
      return top + rowStride > view.top && top < view.bottom;
    };

    for (let i = 0; i < cards.length; i++) {
      const wasOn = onScreen(Math.floor(i / before));
      const isOn = onScreen(Math.floor(i / after));
      if (!wasOn && !isOn) continue;
      const { dx, dy } = slotDelta(i, before, after, colStride, rowStride);
      if (!dx && !dy) continue;
      (cards[i] as HTMLElement).animate?.(
        [
          { transform: `translate(${dx}px, ${dy}px)` },
          { transform: "translate(0, 0)" },
        ],
        { duration: 420, easing: "cubic-bezier(0.33, 1, 0.68, 1)" },
      );
    }
  }, [gridRef, trigger]);
}
