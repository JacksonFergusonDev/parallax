import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  applyUpscale,
  cancelUpscale,
  onUpscaleProgress,
  previewUpscale,
  type Book,
  type UpscalePreview,
  type UpscaleProgress,
} from "../api";

interface Props {
  books: Book[];
  /** Called with the updated book each time an upscaled cover is kept. */
  onChanged: (book: Book) => void;
  onClose: () => void;
}

/**
 * Upscales covers one at a time and asks about each. The next cover is
 * prepared while the current one is being judged; nothing changes in the
 * library unless "Use upscaled" is pressed.
 */
export function UpscaleReview({ books, onChanged, onClose }: Props) {
  const [ready, setReady] = useState<UpscalePreview[]>([]);
  const [position, setPosition] = useState(0);
  const [finished, setFinished] = useState(false);
  const [fullSize, setFullSize] = useState(0);
  const [kept, setKept] = useState(0);
  const [saving, setSaving] = useState(false);
  const [progress, setProgress] = useState<UpscaleProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const ids = useRef(books.map((b) => b.id));

  useEffect(() => {
    let alive = true;
    const unlisten = onUpscaleProgress((p) => alive && setProgress(p));
    (async () => {
      for (const id of ids.current) {
        if (!alive) return;
        try {
          const preview = await previewUpscale(id);
          if (!alive) return;
          if (preview) setReady((r) => [...r, preview]);
          else setFullSize((n) => n + 1);
        } catch (e) {
          if (alive) setError(String(e));
          return;
        }
      }
      if (alive) setFinished(true);
    })();
    return () => {
      alive = false;
      unlisten.then((f) => f());
      void cancelUpscale();
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const current = ready.at(position);
  const book = books.find((b) => b.id === current?.id);
  const done = finished && position >= ready.length;

  const keep = useCallback(async () => {
    if (!current) return;
    setSaving(true);
    setError(null);
    try {
      onChanged(await applyUpscale(current.id, current.run));
      setKept((n) => n + 1);
      setPosition((p) => p + 1);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }, [current, onChanged]);

  const working =
    progress && !finished
      ? `Upscaling ${books.find((b) => b.id === progress.id)?.title ?? "cover"}: ${Math.round((progress.done / progress.total) * 100)}%`
      : !finished && !error
        ? "Starting…"
        : "";

  return (
    <div className="modal-backdrop">
      <div
        className="modal upscale-review"
        role="dialog"
        aria-label="Upscale covers"
      >
        <div className="modal-head">
          <div>
            <h2>Upscale {books.length === 1 ? "cover" : "covers"}</h2>
            <p className="muted">
              {current && book
                ? books.length === 1
                  ? book.title
                  : `${book.title} · ${position + 1} of ${books.length - fullSize}`
                : working}
            </p>
          </div>
          <button className="icon" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>
        {error && <p className="notice notice-error">{error}</p>}
        {current ? (
          <Compare key={`${current.id}-${current.run}`} preview={current} />
        ) : done ? (
          <p className="muted modal-empty">
            {kept === 1 ? "1 cover" : `${kept} covers`} upscaled.
            {fullSize > 0 &&
              ` ${fullSize} ${fullSize === 1 ? "was" : "were"} already full size.`}
          </p>
        ) : (
          !error && (
            <div className="modal-empty upscale-waiting">
              <progress
                value={progress?.done ?? 0}
                max={progress?.total ?? 1}
              />
            </div>
          )
        )}
        <div className="form-actions">
          <span className="muted upscale-credit">
            {current && working ? (
              working
            ) : (
              <>
                Model: 4xBHI_dat2_real by Philip Hofmann, CC BY 4.0, converted
                to Core ML.
              </>
            )}
          </span>
          <span className="spacer" />
          {current ? (
            <>
              <button
                disabled={saving}
                onClick={() => setPosition((p) => p + 1)}
              >
                Keep original
              </button>
              <button className="primary" disabled={saving} onClick={keep}>
                Use upscaled
              </button>
            </>
          ) : (
            <button className={done ? "primary" : ""} onClick={onClose}>
              {done ? "Done" : "Cancel"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/** Before and after stacked, with a divider to drag between them. */
function Compare({ preview }: { preview: UpscalePreview }) {
  const [split, setSplit] = useState(50);
  const [actualSize, setActualSize] = useState(false);
  const [beforeW, beforeH] = preview.before_size;
  const [afterW, afterH] = preview.after_size;
  return (
    <div className="upscale-compare">
      <div className={`compare-stage ${actualSize ? "actual-size" : ""}`}>
        <div
          className="compare-images"
          style={{
            aspectRatio: `${afterW} / ${afterH}`,
            width: actualSize ? `${afterW}px` : undefined,
          }}
        >
          <img src={convertFileSrc(preview.before)} alt="Before" />
          <img
            src={convertFileSrc(preview.after)}
            alt="After"
            style={{ clipPath: `inset(0 0 0 ${split}%)` }}
          />
          <span className="compare-divider" style={{ left: `${split}%` }} />
        </div>
      </div>
      <div className="compare-controls">
        <span className="muted">
          Before {beforeW} × {beforeH}
        </span>
        <input
          type="range"
          min={0}
          max={100}
          value={split}
          aria-label="Compare before and after"
          onChange={(e) => setSplit(Number(e.target.value))}
        />
        <span className="muted">
          After {afterW} × {afterH}
        </span>
        <button className="link" onClick={() => setActualSize((a) => !a)}>
          {actualSize ? "Fit" : "Actual size"}
        </button>
      </div>
    </div>
  );
}
