import { useCallback, useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  cleanImages,
  koboHealth,
  mergeCollections,
  removeFromDevice,
  type KoboHealth,
} from "../api";
import { formatSize } from "../duplicates";

interface Props {
  mount: string;
  onChanged: () => void;
  onClose: () => void;
}

const plural = (n: number, word: string, many = `${word}s`) =>
  `${n} ${n === 1 ? word : many}`;

/** Finds and fixes the small problems Kobo Utilities users check for. */
export function KoboCheck({ mount, onChanged, onClose }: Props) {
  const [health, setHealth] = useState<KoboHealth | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{
    error: boolean;
    text: string;
  } | null>(null);

  const load = useCallback(
    () =>
      koboHealth(mount)
        .then(setHealth)
        .catch((e) => setMessage({ error: true, text: String(e) })),
    [mount],
  );
  useEffect(() => {
    load();
  }, [load]);

  const fix = async (action: () => Promise<string>) => {
    setBusy(true);
    setMessage(null);
    try {
      setMessage({ error: false, text: await action() });
      onChanged();
      await load();
    } catch (e) {
      setMessage({ error: true, text: String(e) });
    } finally {
      setBusy(false);
    }
  };

  const removeEntries = async () => {
    if (!health) return;
    const n = health.missing_files.length;
    const ok = await ask(
      `Remove ${plural(n, "entry", "entries")} for books whose file is gone? Their reading progress and highlights are deleted too. A backup of the Kobo's database is made first.`,
      {
        title: "Remove entries",
        kind: "warning",
        okLabel: "Remove",
        cancelLabel: "Cancel",
      },
    );
    if (!ok) return;
    fix(async () => {
      const outcomes = await removeFromDevice(
        mount,
        health.missing_files.map((m) => m.content_id),
      );
      const failed = outcomes.filter((o) => "Err" in o.result).length;
      if (failed) throw new Error(`${failed} could not be removed.`);
      return `Removed ${plural(n, "entry", "entries")}.`;
    });
  };

  return (
    <div className="modal-backdrop" onClick={busy ? undefined : onClose}>
      <div className="modal check" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <div>
            <h2>Check this Kobo</h2>
            <p className="muted">
              {health
                ? "Small problems that build up over time, and how to fix them."
                : "Checking…"}
            </p>
          </div>
        </div>
        {health && (
          <ul className="check-items">
            <li>
              <div>
                <strong>
                  {health.missing_files.length
                    ? `${plural(health.missing_files.length, "book")} whose file is gone`
                    : "Every book's file is there"}
                </strong>
                <p className="muted">
                  These show in the Kobo's library but won't open.
                </p>
                {health.missing_files.slice(0, 8).map((m) => (
                  <p key={m.content_id} className="check-detail">
                    {m.title || m.path}
                  </p>
                ))}
              </div>
              {health.missing_files.length > 0 && (
                <button
                  className="secondary"
                  disabled={busy}
                  onClick={removeEntries}
                >
                  Remove entries
                </button>
              )}
            </li>
            <li>
              <div>
                <strong>
                  {health.not_imported.length
                    ? `${plural(health.not_imported.length, "file")} not in the Kobo's library yet`
                    : "Every book file is in the Kobo's library"}
                </strong>
                {health.not_imported.length > 0 && (
                  <p className="muted">
                    Eject the Kobo to let it add them. If one stays here
                    afterwards, the Kobo couldn't read it.
                  </p>
                )}
                {health.not_imported.slice(0, 8).map((m) => (
                  <p key={m.path} className="check-detail">
                    {m.path}
                  </p>
                ))}
              </div>
            </li>
            <li>
              <div>
                <strong>
                  {health.duplicate_collections.length
                    ? `${plural(health.duplicate_collections.length, "collection")} listed twice`
                    : "No duplicated collections"}
                </strong>
                {health.duplicate_collections.length > 0 && (
                  <p className="muted">
                    {health.duplicate_collections
                      .map((d) => `${d.name} (${d.copies}×)`)
                      .join(", ")}
                    . Merging keeps the oldest copy; no book leaves a
                    collection.
                  </p>
                )}
              </div>
              {health.duplicate_collections.length > 0 && (
                <button
                  className="secondary"
                  disabled={busy}
                  onClick={() =>
                    fix(async () => {
                      const n = await mergeCollections(mount);
                      return `Merged: removed ${plural(n, "extra copy", "extra copies")}.`;
                    })
                  }
                >
                  Merge
                </button>
              )}
            </li>
            <li>
              <div>
                <strong>
                  {health.orphaned_images
                    ? `${plural(health.orphaned_images, "leftover cover image")} (${formatSize(health.orphaned_image_bytes)})`
                    : "No leftover cover images"}
                </strong>
                {health.orphaned_images > 0 && (
                  <p className="muted">
                    Cached covers of books no longer on the Kobo.
                  </p>
                )}
              </div>
              {health.orphaned_images > 0 && (
                <button
                  className="secondary"
                  disabled={busy}
                  onClick={() =>
                    fix(async () => {
                      const [files, bytes] = await cleanImages(mount);
                      return `Deleted ${plural(files, "image")}, freeing ${formatSize(bytes)}.`;
                    })
                  }
                >
                  Delete
                </button>
              )}
            </li>
          </ul>
        )}
        {message && (
          <p className={`notice notice-${message.error ? "error" : "info"}`}>
            {message.text}
          </p>
        )}
        <div className="form-actions">
          <button className="primary" onClick={onClose} disabled={busy}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
