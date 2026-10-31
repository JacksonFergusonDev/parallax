import { useEffect, useState } from "react";
import { changeCollections, folderGroups, type FolderGroup } from "../api";

interface Props {
  mount: string;
  onDone: (created: number) => void;
  onClose: () => void;
}

/** Turns the Kobo's folders into collections, which Nickel otherwise ignores. */
export function FolderImport({ mount, onDone, onClose }: Props) {
  const [groups, setGroups] = useState<FolderGroup[] | null>(null);
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    folderGroups(mount)
      .then((g) => {
        if (!alive) return;
        setGroups(g);
        setChosen(new Set(g.map((x) => x.folder)));
      })
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [mount]);

  const toggle = (folder: string) =>
    setChosen((prev) => {
      const next = new Set(prev);
      if (next.has(folder)) next.delete(folder);
      else next.add(folder);
      return next;
    });

  const create = async () => {
    const picked = (groups ?? []).filter((g) => chosen.has(g.folder));
    setBusy(true);
    setError(null);
    try {
      await changeCollections(
        mount,
        picked.map((g) => ({
          kind: "add",
          name: g.folder,
          content_ids: g.content_ids,
        })),
      );
      onDone(picked.length);
      onClose();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal small" onClick={(e) => e.stopPropagation()}>
        <h2>Folders to collections</h2>
        <p className="muted">
          Your Kobo ignores folders. Make each folder a collection you can
          browse on the device. Books stay where they are.
        </p>
        {groups === null && !error && <p className="muted">Reading folders…</p>}
        {groups?.length === 0 && (
          <p className="muted">There are no books in folders on this Kobo.</p>
        )}
        <ul className="check-list">
          {groups?.map((g) => (
            <li key={g.folder}>
              <label>
                <input
                  type="checkbox"
                  checked={chosen.has(g.folder)}
                  onChange={() => toggle(g.folder)}
                />
                <span className="grow">{g.folder}</span>
                <span className="muted">
                  {g.content_ids.length}{" "}
                  {g.content_ids.length === 1 ? "book" : "books"}
                </span>
              </label>
            </li>
          ))}
        </ul>
        {error && <p className="notice notice-error">{error}</p>}
        <div className="form-actions">
          <button className="secondary" onClick={onClose}>
            Cancel
          </button>
          <button
            className="primary"
            disabled={busy || chosen.size === 0}
            onClick={create}
          >
            {busy
              ? "Creating…"
              : `Create ${chosen.size} ${chosen.size === 1 ? "collection" : "collections"}`}
          </button>
        </div>
      </div>
    </div>
  );
}
