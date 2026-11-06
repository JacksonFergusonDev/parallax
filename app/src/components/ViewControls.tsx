import { useEffect, useState } from "react";
import { LayoutGrid, List } from "lucide-react";
import { nextSort, parseSort, type Column, type Sort } from "../sort";

export type ViewMode = "grid" | "list";

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** A view mode and sort remembered per screen; storage is optional. */
export function useViewState<K extends string>(
  screen: string,
  keys: readonly K[],
  defaultMode: ViewMode,
) {
  const [mode, setMode] = useState<ViewMode>(() => {
    const saved = read(`${screen}-view`);
    return saved === "grid" || saved === "list" ? saved : defaultMode;
  });
  const [sort, setSort] = useState<Sort<K>>(() =>
    parseSort(read(`${screen}-sort`), keys),
  );
  useEffect(() => {
    try {
      localStorage.setItem(`${screen}-view`, mode);
      localStorage.setItem(`${screen}-sort`, JSON.stringify(sort));
    } catch {
      /* storage unavailable */
    }
  }, [screen, mode, sort]);
  return { mode, setMode, sort, setSort };
}

interface ControlsProps<T, K extends string> {
  mode: ViewMode;
  onMode: (mode: ViewMode) => void;
  sort: Sort<K>;
  onSort: (sort: Sort<K>) => void;
  columns: readonly Column<T, K>[];
  /** Shown beside the sort picker, such as a sort still being prepared. */
  note?: string | null;
}

/** View toggle, plus a sort picker in the grid (the list sorts by header). */
export function ViewControls<T, K extends string>({
  mode,
  onMode,
  sort,
  onSort,
  columns,
  note,
}: ControlsProps<T, K>) {
  return (
    <div className="view-controls">
      {mode === "grid" && (
        <>
          {note && <span className="muted sort-note">{note}</span>}
          <select
            className="sort-select"
            aria-label="Sort by"
            value={sort?.key ?? ""}
            onChange={(e) =>
              onSort(
                e.target.value
                  ? { key: e.target.value as K, dir: "desc" }
                  : null,
              )
            }
          >
            <option value="">Default order</option>
            {columns.map((c) => (
              <option key={c.key} value={c.key}>
                {c.label}
              </option>
            ))}
          </select>
          {sort && (
            <button
              className="icon sort-dir"
              title={sort.dir === "desc" ? "Descending" : "Ascending"}
              aria-label={`Sort ${sort.dir === "desc" ? "ascending" : "descending"} instead`}
              onClick={() =>
                onSort({ ...sort, dir: sort.dir === "desc" ? "asc" : "desc" })
              }
            >
              {sort.dir === "desc" ? "↓" : "↑"}
            </button>
          )}
        </>
      )}
      <div className="segmented" role="group" aria-label="View">
        {(["grid", "list"] as const).map((m) => (
          <button
            key={m}
            aria-pressed={mode === m}
            className={mode === m ? "active" : ""}
            title={m === "grid" ? "Covers" : "List"}
            onClick={() => onMode(m)}
          >
            {m === "grid" ? <LayoutGrid /> : <List />}
            {m === "grid" ? "Covers" : "List"}
          </button>
        ))}
      </div>
    </div>
  );
}

interface HeaderProps<T, K extends string> {
  columns: readonly Column<T, K>[];
  sort: Sort<K>;
  onSort: (sort: Sort<K>) => void;
}

/** List header: each click cycles descending → ascending → default. */
export function SortHeader<T, K extends string>({
  columns,
  sort,
  onSort,
}: HeaderProps<T, K>) {
  return (
    <div className="list-row header" role="row">
      <span />
      {columns.map((c) => {
        const dir = sort?.key === c.key ? sort.dir : null;
        return (
          <button
            key={c.key}
            role="columnheader"
            aria-sort={
              dir === "desc"
                ? "descending"
                : dir === "asc"
                  ? "ascending"
                  : "none"
            }
            className={`col-head ${c.align === "end" ? "end" : ""} ${dir ? "sorted" : ""}`}
            onClick={() => onSort(nextSort(sort, c.key))}
          >
            {c.label}
            <span className="sort-arrow" aria-hidden>
              {dir === "desc" ? "↓" : dir === "asc" ? "↑" : ""}
            </span>
          </button>
        );
      })}
    </div>
  );
}

/** CSS grid tracks for a list: thumbnail then the columns. */
export const listTracks = <T, K extends string>(
  columns: readonly Column<T, K>[],
) => ["32px", ...columns.map((c) => c.width)].join(" ");
