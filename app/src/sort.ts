// Tri-state column sorting shared by the library and Kobo views: the first
// click on a column sorts descending, the second ascending, the third goes
// back to the view's default order.

export type SortDir = "desc" | "asc";
export type Sort<K extends string> = { key: K; dir: SortDir } | null;

/** A column's sort value; arrays compare element by element. null = missing. */
export type SortValue = string | number | (string | number)[] | null;

export interface Column<T, K extends string> {
  key: K;
  label: string;
  /** CSS grid track for the list view. */
  width: string;
  align?: "end";
  value: (item: T) => SortValue;
}

export function nextSort<K extends string>(current: Sort<K>, key: K): Sort<K> {
  if (!current || current.key !== key) return { key, dir: "desc" };
  return current.dir === "desc" ? { key, dir: "asc" } : null;
}

export const collator = new Intl.Collator(undefined, {
  sensitivity: "base",
  numeric: true,
});

function compare(a: string | number, b: string | number): number {
  return typeof a === "number" && typeof b === "number"
    ? a - b
    : collator.compare(String(a), String(b));
}

function compareValues(a: SortValue, b: SortValue): number {
  const xs = Array.isArray(a) ? a : [a ?? ""];
  const ys = Array.isArray(b) ? b : [b ?? ""];
  for (let i = 0; i < Math.min(xs.length, ys.length); i++) {
    const c = compare(xs[i], ys[i]);
    if (c) return c;
  }
  return xs.length - ys.length;
}

/**
 * Sorts `items` (already in the default order) by `sort`. Missing values go
 * last in both directions; ties keep the default order.
 */
export function applySort<T, K extends string>(
  items: T[],
  sort: Sort<K>,
  columns: readonly Column<T, K>[],
): T[] {
  const column = sort && columns.find((c) => c.key === sort.key);
  if (!sort || !column) return items;
  const sign = sort.dir === "asc" ? 1 : -1;
  return items
    .map((item, index) => ({ item, index, v: column.value(item) }))
    .sort((a, b) => {
      if (a.v === null || b.v === null) {
        if (a.v !== b.v) return a.v === null ? 1 : -1;
        return a.index - b.index;
      }
      return sign * compareValues(a.v, b.v) || a.index - b.index;
    })
    .map((x) => x.item);
}

/** Parses a stored sort, dropping it if the column no longer exists. */
export function parseSort<K extends string>(
  raw: string | null,
  keys: readonly K[],
): Sort<K> {
  if (!raw) return null;
  try {
    const s = JSON.parse(raw) as { key?: unknown; dir?: unknown };
    if (keys.includes(s.key as K) && (s.dir === "asc" || s.dir === "desc"))
      return { key: s.key as K, dir: s.dir };
  } catch {
    /* corrupt value */
  }
  return null;
}
