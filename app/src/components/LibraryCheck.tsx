import { useEffect, useState } from "react";
import {
  libraryCheck,
  onCheckProgress,
  type Book,
  type Finding,
  type ImportProgress,
  upscaleAvailable,
} from "../api";
import { groupFindings } from "../checks";
import { ReviewOnAdd } from "./ImportReview";

interface Props {
  books: Map<number, Book>;
  /** Selects a book so its edit panel opens. */
  onShow: (id: number) => void;
  /** Starts reviewing upscaled versions of these books' covers. */
  onUpscale: (ids: number[]) => void;
  onClose: () => void;
}

/** Checks every book and lists what's worth fixing. */
export function LibraryCheck({ books, onShow, onUpscale, onClose }: Props) {
  const [findings, setFindings] = useState<Finding[] | null>(null);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showNotes, setShowNotes] = useState(false);
  const [canUpscale, setCanUpscale] = useState(false);

  useEffect(() => {
    let alive = true;
    upscaleAvailable()
      .then((available) => alive && setCanUpscale(available))
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    let alive = true;
    const unlisten = onCheckProgress((p) => alive && setProgress(p));
    libraryCheck()
      .then((f) => alive && setFindings(f))
      .catch((e) => alive && setError(String(e)))
      .finally(() => unlisten.then((f) => f()));
    return () => {
      alive = false;
      unlisten.then((f) => f());
    };
  }, []);

  const groups = findings ? groupFindings(findings) : [];
  const problems = groups.filter((g) => !g.info.note);
  const notes = groups.filter((g) => g.info.note);
  const shown = showNotes ? groups : problems;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal library-check" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <div>
            <h2>Check library</h2>
            <p className="muted">
              {findings === null
                ? progress
                  ? `Checking ${progress.index} of ${progress.total}…`
                  : "Checking…"
                : problems.length
                  ? `${problems.reduce((n, g) => n + g.findings.length, 0)} things worth fixing. Click a book to open it.`
                  : "Nothing to fix."}
            </p>
          </div>
        </div>
        {error && <p className="notice notice-error">{error}</p>}
        {findings && (
          <div className="check-groups">
            {shown.map((g) => (
              <section key={g.kind} className={g.info.note ? "note" : ""}>
                <h3>
                  {g.info.label}{" "}
                  <span className="muted">{g.findings.length}</span>
                </h3>
                <p className="muted">{g.info.help}</p>
                {g.kind === "small_cover" && canUpscale && (
                  <UpscaleThese
                    ids={g.findings
                      .flatMap((f) => f.book_ids)
                      .filter((id) => !books.get(id)?.cover_upscaled)}
                    onUpscale={onUpscale}
                  />
                )}
                <ul>
                  {g.findings.map((f, i) => (
                    <li key={i}>
                      {f.book_ids.length === 1 ? (
                        <button
                          className="link"
                          onClick={() => onShow(f.book_ids[0])}
                        >
                          {books.get(f.book_ids[0])?.title ??
                            `Book ${f.book_ids[0]}`}
                        </button>
                      ) : null}
                      {f.detail && <span className="muted"> {f.detail}</span>}
                    </li>
                  ))}
                </ul>
              </section>
            ))}
          </div>
        )}
        <div className="form-actions">
          {notes.length > 0 && (
            <button className="link" onClick={() => setShowNotes((s) => !s)}>
              {showNotes
                ? "Hide notes"
                : `Show ${notes.reduce((n, g) => n + g.findings.length, 0)} notes`}
            </button>
          )}
          <ReviewOnAdd />
          <span className="spacer" />
          <button className="primary" onClick={onClose} autoFocus>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

/** Starts upscaling the covers that can still be improved, if any. */
function UpscaleThese({
  ids,
  onUpscale,
}: {
  ids: number[];
  onUpscale: (ids: number[]) => void;
}) {
  if (!ids.length) return null;
  return (
    <button className="link" onClick={() => onUpscale(ids)}>
      Upscale and review these…
    </button>
  );
}
