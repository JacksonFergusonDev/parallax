import { useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  deviceBackups,
  deviceRestore,
  libraryRestore,
  librarySnapshots,
  type BackupInfo,
  type DeviceInfo,
  type LogEntry,
  type Snapshot,
} from "../api";
import { plural } from "../importReview";

export type RevertScope = "library" | "kobo";

interface Props {
  /** The connected Kobo, if there is one. */
  device: DeviceInfo | null;
  start: RevertScope;
  /** Going back worked; `text` says what happened. */
  onReverted: (scope: RevertScope, text: string) => void;
  onClose: () => void;
}

interface Point {
  file: string;
  at: string;
  what: string;
  note?: string;
  blocked: string | null;
}

const when = (iso: string) =>
  new Date(iso).toLocaleString(undefined, {
    weekday: "short",
    day: "numeric",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  });

/** "KEpub/Dune - Frank Herbert.kepub.epub" → "Dune - Frank Herbert". */
const bookName = (contentId: string) =>
  (contentId.split("/").pop() ?? contentId).replace(/(\.kepub)?\.epub$/i, "");

const KOBO_OPS: Record<string, string> = {
  send: "Sent",
  replace: "Updated",
  update_details: "Updated details of",
  remove: "Removed",
  collection: "Changed collections for",
  series: "Changed series for",
  "reading status": "Changed reading status for",
};

/** One line for the change a Kobo backup was taken before. */
export function describeEntries(entries: LogEntry[]): string {
  if (!entries.length) return "An earlier change";
  if (entries.some((e) => e.op === "restore")) return "Reverting changes";
  const ops = [...new Set(entries.map((e) => e.op))];
  const text = ops
    .map((op) => {
      const ids = entries
        .filter((e) => e.op === op)
        .flatMap((e) => e.content_ids ?? []);
      const label = KOBO_OPS[op] ?? `Changed ${op} for`;
      const books = new Set(ids);
      if (books.size === 0) return label.replace(/ for$/, "");
      return books.size === 1
        ? `${label} ${bookName(ids[0])}`
        : `${label} ${plural(books.size, "book")}`;
    })
    .join(", ");
  return entries.every((e) => e.error) ? `${text} (failed)` : text;
}

const LIBRARY_OPS: Record<string, string> = {
  remove: "Removed",
  tidy: "Tidied genres",
  enhance: "Added details to",
  calibre: "Imported from calibre:",
  "kobo-import": "Added from Kobo:",
};

export function describeSnapshot(s: Snapshot): string {
  if (s.op === "restore") return "Reverting changes";
  const label = LIBRARY_OPS[s.op] ?? s.op;
  return s.detail ? `${label} ${s.detail}` : label;
}

const fromBackup = (b: BackupInfo): Point => ({
  file: b.file,
  at: b.at,
  what: describeEntries(b.entries),
  note: b.removed_files
    ? `${plural(b.removed_files, "removed book")} can come back`
    : undefined,
  blocked: b.blocked ?? null,
});

const fromSnapshot = (s: Snapshot): Point => ({
  file: s.file,
  at: s.at,
  what: describeSnapshot(s),
  note: `${plural(s.books, "book")} in the library then`,
  blocked: s.blocked ?? null,
});

function libraryText(r: {
  restored: number;
  set_aside: number;
  lost: string[];
}): string {
  const parts = ["Library reverted."];
  if (r.restored) parts.push(`${plural(r.restored, "book")} brought back.`);
  if (r.set_aside)
    parts.push(`${plural(r.set_aside, "book")} added since set aside.`);
  if (r.lost.length)
    parts.push(`Files no longer held for: ${r.lost.join(", ")}.`);
  return parts.join(" ");
}

/** Lists the backups of the library and the Kobo, and goes back to one. */
export function RevertChanges({ device, start, onReverted, onClose }: Props) {
  const [scope, setScope] = useState<RevertScope>(device ? start : "library");
  const [points, setPoints] = useState<Point[] | null>(null);
  const [chosen, setChosen] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const mount = device?.mount;

  useEffect(() => {
    let alive = true;
    const load =
      scope === "kobo" && mount
        ? deviceBackups(mount).then((b) => b.map(fromBackup))
        : librarySnapshots().then((s) => s.map(fromSnapshot));
    load
      .then((p) => alive && setPoints(p))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [scope, mount]);

  const showScope = (next: RevertScope) => {
    if (next === scope) return;
    setScope(next);
    setPoints(null);
    setChosen(null);
    setError(null);
  };

  const point = points?.find((p) => p.file === chosen);
  const readOnly = scope === "kobo" ? device?.read_only : null;

  const revert = async () => {
    if (!point) return;
    const kobo = scope === "kobo";
    const ok = await ask(
      kobo
        ? `Put your Kobo back to how it was before this change on ${when(point.at)}?\n\nReading progress, highlights and collections since then are replaced by what they were. Books added since stay on the Kobo but are imported again, so they start from the beginning.\n\nHow the Kobo is now is saved first, so you can undo this.`
        : `Put your library back to how it was before this change on ${when(point.at)}?\n\nEdits since then are undone, and books added since are set aside for 30 days. Your settings stay as they are.\n\nHow the library is now is saved first, so you can undo this.`,
      {
        title: "Revert changes",
        kind: "warning",
        okLabel: "Revert",
        cancelLabel: "Cancel",
      },
    );
    if (!ok) return;
    setBusy(true);
    setError(null);
    try {
      if (kobo && mount) {
        const r = await deviceRestore(mount, point.file);
        const files = r.files_restored
          ? ` ${plural(r.files_restored, "book")} put back.`
          : "";
        onReverted(
          "kobo",
          `Kobo reverted.${files} Eject to let your Kobo refresh its library.`,
        );
      } else {
        onReverted("library", libraryText(await libraryRestore(point.file)));
      }
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  return (
    <div className="modal-backdrop" onClick={busy ? undefined : onClose}>
      <div className="modal revert" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <div>
            <h2>Revert changes</h2>
            <p className="muted">
              Each point is how things were just before that change. Choose one
              to go back to it.
            </p>
          </div>
        </div>
        {device && (
          <div className="segmented" role="group" aria-label="What to revert">
            {(["library", "kobo"] as const).map((s) => (
              <button
                key={s}
                className={scope === s ? "active" : ""}
                aria-pressed={scope === s}
                disabled={busy}
                onClick={() => showScope(s)}
              >
                {s === "library" ? "Library" : device.model}
              </button>
            ))}
          </div>
        )}
        {error && <p className="notice notice-error">{error}</p>}
        {readOnly && (
          <p className="notice notice-info">
            This Kobo can't be changed: {readOnly}
          </p>
        )}
        {points === null && !error && <p className="muted">Loading…</p>}
        {points?.length === 0 && (
          <p className="muted">
            Nothing to go back to yet. A point is saved before each change to
            {scope === "kobo"
              ? " your Kobo."
              : " your library that removes books or changes many at once."}
          </p>
        )}
        {points && points.length > 0 && (
          <ul className="revert-points">
            {points.map((p) => (
              <li key={p.file}>
                <button
                  className="revert-point"
                  aria-pressed={p.file === chosen}
                  disabled={busy || p.blocked !== null}
                  onClick={() => setChosen(p.file)}
                >
                  <span className="revert-when muted">{when(p.at)}</span>
                  <span className="revert-what">Before: {p.what}</span>
                  {(p.blocked ?? p.note) && (
                    <span className="revert-note muted">
                      {p.blocked ? `Can't use: ${p.blocked}` : p.note}
                    </span>
                  )}
                </button>
              </li>
            ))}
          </ul>
        )}
        <div className="form-actions">
          <button className="secondary" onClick={onClose} disabled={busy}>
            Cancel
          </button>
          <button
            className="primary"
            onClick={revert}
            disabled={!point || busy || Boolean(readOnly)}
          >
            {busy ? "Reverting…" : "Revert…"}
          </button>
        </div>
      </div>
    </div>
  );
}
