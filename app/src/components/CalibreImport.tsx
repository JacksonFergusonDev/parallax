import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  calibreDefaultLibrary,
  calibreImport,
  calibrePreview,
  onImportProgress,
  type CalibreImportOutcome,
  type CalibrePreview,
  type ImportProgress,
} from "../api";

interface Props {
  onImported: () => void;
  onClose: () => void;
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;

/** Moves a calibre library in: pick the folder, see what happens, import. */
export function CalibreImport({ onImported, onClose }: Props) {
  /** undefined while looking for calibre's library; null when none found. */
  const [path, setPath] = useState<string | null | undefined>(undefined);
  const [preview, setPreview] = useState<CalibrePreview | null>(null);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [importing, setImporting] = useState(false);
  const [outcomes, setOutcomes] = useState<CalibreImportOutcome[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    calibreDefaultLibrary()
      .then((p) => alive && setPath(p))
      .catch(() => alive && setPath(null));
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    if (!path) return;
    let alive = true;
    calibrePreview(path)
      .then((p) => alive && setPreview(p))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [path]);

  const choose = async () => {
    const picked = await open({
      directory: true,
      title: "Choose your calibre library folder",
      defaultPath: path ?? undefined,
    });
    if (typeof picked === "string") {
      setOutcomes(null);
      setPreview(null);
      setError(null);
      setPath(picked);
    }
  };

  const run = async () => {
    if (!path) return;
    setImporting(true);
    setError(null);
    const unlisten = await onImportProgress(setProgress);
    try {
      setOutcomes(await calibreImport(path));
      onImported();
    } catch (e) {
      setError(String(e));
    } finally {
      unlisten();
      setImporting(false);
    }
  };

  const count = (kind: string) =>
    outcomes?.filter((o) => "Ok" in o.result && o.result.Ok.kind === kind)
      .length ?? 0;
  const problems = (outcomes ?? []).filter(
    (o) =>
      "Err" in o.result || ("Ok" in o.result && o.result.Ok.kind === "skipped"),
  );

  return (
    <div className="modal-backdrop" onClick={importing ? undefined : onClose}>
      <div
        className="modal calibre-import"
        role="dialog"
        aria-label="Import from calibre"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <div>
            <h2>Import from calibre</h2>
            <p className="muted">
              Copies your calibre books (EPUB, or KEPUB when that's all there
              is) with calibre's titles, authors, series, tags and covers.
              calibre's library isn't changed, and books already here aren't
              added twice.
            </p>
          </div>
        </div>

        <div className="calibre-path">
          <span className="muted">Library</span>
          <code title={path ?? undefined}>
            {path === undefined
              ? "Looking for calibre…"
              : (path ?? "No calibre library found")}
          </code>
          <button className="secondary" onClick={choose} disabled={importing}>
            {path ? "Choose another…" : "Choose folder…"}
          </button>
        </div>

        {error && <p className="notice notice-error wrap">{error}</p>}

        {outcomes ? (
          <div className="calibre-result">
            <p>
              {plural(count("added"), "book")} added
              {count("matched") > 0 &&
                `, ${count("matched")} already in your library`}
              {problems.length > 0 && `, ${problems.length} not imported`}.
            </p>
            {problems.length > 0 && (
              <ul className="calibre-problems">
                {problems.map((o) => (
                  <li key={o.calibre_id}>
                    {o.title}{" "}
                    <span className="muted">
                      {"Err" in o.result
                        ? o.result.Err
                        : o.result.Ok.kind === "skipped"
                          ? o.result.Ok.reason
                          : ""}
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        ) : importing ? (
          <p className="muted">
            {progress
              ? `Importing ${progress.index} of ${progress.total}: ${progress.title}…`
              : "Starting…"}
          </p>
        ) : preview ? (
          <ul className="calibre-preview">
            <li>
              <strong>{plural(preview.new, "book")}</strong> to import
            </li>
            {preview.already > 0 && (
              <li className="muted">
                {preview.already} already in your library
              </li>
            )}
            {preview.no_epub.map(([formats, n]) => (
              <li key={formats} className="muted">
                {n} without an EPUB ({formats}), skipped
              </li>
            ))}
          </ul>
        ) : (
          path && !error && <p className="muted">Reading calibre's library…</p>
        )}

        <div className="form-actions">
          <span className="spacer" />
          {outcomes ? (
            <button className="primary" onClick={onClose} autoFocus>
              Done
            </button>
          ) : (
            <>
              <button
                className="secondary"
                onClick={onClose}
                disabled={importing}
              >
                Cancel
              </button>
              <button
                className="primary"
                onClick={run}
                disabled={!preview || preview.new === 0 || importing}
              >
                {importing
                  ? "Importing…"
                  : preview && preview.new > 0
                    ? `Import ${plural(preview.new, "book")}`
                    : "Import"}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
