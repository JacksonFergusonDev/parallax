import { useCallback, useEffect, useState } from "react";
import { getSetting, onSettingChange, setSetting } from "../api";
import { isFiltered, type LibraryFilter } from "../libraryFilter";
import {
  activeSaved,
  parseSaved,
  SAVED_SEARCHES,
  SAVED_SEARCHES_ON,
  serializeSaved,
  withSaved,
  type SavedSearch,
} from "../savedSearches";
import { NameDialog } from "./NameDialog";

/** Whether saved searches are on, and what they are, kept current with
 * Settings. */
function useSavedSearches() {
  const [on, setOn] = useState(false);
  const [list, setList] = useState<SavedSearch[]>([]);
  useEffect(() => {
    let alive = true;
    void getSetting(SAVED_SEARCHES_ON)
      .then((v) => alive && setOn(v === "on"))
      .catch(() => {});
    void getSetting(SAVED_SEARCHES)
      .then((v) => alive && setList(parseSaved(v)))
      .catch(() => {});
    const stop = onSettingChange(({ key, value }) => {
      if (key === SAVED_SEARCHES_ON) setOn(value === "on");
      if (key === SAVED_SEARCHES) setList(parseSaved(value));
    });
    return () => {
      alive = false;
      void stop.then((f) => f());
    };
  }, []);
  const save = useCallback(async (next: SavedSearch[]) => {
    await setSetting(SAVED_SEARCHES, serializeSaved(next));
    setList(next);
  }, []);
  return { on, list, save };
}

/** Pick a saved search, save the one showing, or forget it. Shown when
 * turned on in Settings › Library. */
export function SavedSearches({
  query,
  filter,
  onApply,
}: {
  query: string;
  filter: LibraryFilter;
  onApply: (search: SavedSearch) => void;
}) {
  const { on, list, save } = useSavedSearches();
  const [naming, setNaming] = useState(false);
  if (!on) return null;
  const active = activeSaved(list, query, filter);
  const something = query.trim() !== "" || isFiltered(filter);
  return (
    <>
      {list.length > 0 && (
        <select
          className={`sort-select filter-select ${active ? "active" : ""}`}
          aria-label="Saved searches"
          value={active?.name ?? ""}
          onChange={(e) => {
            const picked = list.find((s) => s.name === e.target.value);
            if (picked) onApply(picked);
          }}
        >
          <option value="" disabled>
            Saved searches
          </option>
          {list.map((s) => (
            <option key={s.name} value={s.name}>
              {s.name}
            </option>
          ))}
        </select>
      )}
      {active ? (
        <button
          className="link"
          onClick={() =>
            void save(list.filter((s) => s.name !== active.name)).catch(
              console.error,
            )
          }
        >
          Forget “{active.name}”
        </button>
      ) : (
        something && (
          <button className="link" onClick={() => setNaming(true)}>
            Save search…
          </button>
        )
      )}
      {naming && (
        <NameDialog
          title="Save this search"
          confirmLabel="Save"
          onClose={() => setNaming(false)}
          onSubmit={(name) =>
            save(withSaved(list, { name, query: query.trim(), filter }))
          }
        />
      )}
    </>
  );
}
