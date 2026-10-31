import { useEffect, useState } from "react";
import { coverDimensions } from "../readCache";
import { searchCovers, type Book, type CoverCandidate } from "../api";

/** Covers shorter than this look soft on a Kobo screen (1680 px tall). */
const SHARP_HEIGHT = 1200;

interface Props {
  book: Book;
  onPick: (url: string) => Promise<void>;
  onClose: () => void;
}

export function CoverPicker({ book, onPick, onClose }: Props) {
  const [candidates, setCandidates] = useState<CoverCandidate[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sizes, setSizes] = useState<Record<string, [number, number]>>({});
  const [broken, setBroken] = useState<Set<string>>(new Set());
  const [picking, setPicking] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    searchCovers(book.id)
      .then((c) => alive && setCandidates(c))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [book.id]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  // Measure the full-size image, which is what gets downloaded.
  useEffect(() => {
    let alive = true;
    for (const c of candidates ?? []) {
      coverDimensions(c.url)
        .then((size) => alive && setSizes((s) => ({ ...s, [c.url]: size })))
        .catch(() => alive && setBroken((b) => new Set(b).add(c.url)));
    }
    return () => {
      alive = false;
    };
  }, [candidates]);

  const pick = async (url: string) => {
    setPicking(url);
    setError(null);
    try {
      await onPick(url);
      onClose();
    } catch (e) {
      setError(String(e));
      setPicking(null);
    }
  };

  const shown = (candidates ?? []).filter((c) => !broken.has(c.url));
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-label="Choose a cover"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <div>
            <h2>Choose a cover</h2>
            <p className="muted">
              {book.title}
              {book.authors[0] ? ` · ${book.authors[0]}` : ""}
            </p>
          </div>
          <button className="icon" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>
        {error && <p className="notice notice-error">{error}</p>}
        {candidates === null && !error ? (
          <p className="muted modal-empty">
            Searching Apple Books, Open Library and Google Books…
          </p>
        ) : shown.length === 0 && !error ? (
          <p className="muted modal-empty">
            No covers found. Try editing the title or author first.
          </p>
        ) : (
          <div className="picker-grid">
            {shown.map((c) => {
              const size = sizes[c.url];
              const sharp = size && size[1] >= SHARP_HEIGHT;
              return (
                <button
                  key={c.url}
                  className={`picker-item ${picking === c.url ? "picking" : ""}`}
                  disabled={picking !== null}
                  onClick={() => pick(c.url)}
                >
                  <img src={c.preview_url} alt="" draggable={false} />
                  <span className="picker-meta">
                    <span
                      className={
                        size ? (sharp ? "size sharp" : "size soft") : "size"
                      }
                    >
                      {size ? `${size[0]} × ${size[1]}` : "…"}
                    </span>
                    <span className="muted">{c.source}</span>
                  </span>
                  {picking === c.url && (
                    <span className="overlay">Downloading…</span>
                  )}
                </button>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
