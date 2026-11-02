// Click selection shared by every list and grid of books: a click selects
// just that item (or clears it if it was the only one), ⌘-click adds or
// removes one, Shift-click adds the range from the last item clicked.

export interface Modifiers {
  shiftKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
}

/** How to pick several, shown wherever items can be selected. */
export const SELECTION_HINT =
  "⌘-click to select several · Shift-click for a range";

/** The selection after clicking `id`; `anchor` is the last item clicked and is updated. */
export function clickSelect<T>(
  prev: ReadonlySet<T>,
  id: T,
  order: readonly T[],
  anchor: { current: T | null },
  e: Modifiers,
): Set<T> {
  if (e.shiftKey && anchor.current !== null) {
    const [a, b] = [order.indexOf(anchor.current), order.indexOf(id)].sort(
      (x, y) => x - y,
    );
    if (a >= 0) return new Set([...prev, ...order.slice(a, b + 1)]);
  }
  anchor.current = id;
  if (e.metaKey || e.ctrlKey) {
    const next = new Set(prev);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return next;
  }
  return prev.size === 1 && prev.has(id) ? new Set() : new Set([id]);
}
