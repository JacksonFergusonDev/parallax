import { useState } from "react";
import type { Book } from "../api";
import { GoodreadsView } from "./GoodreadsView";
import { ReadingView } from "./ReadingView";

export type ReadingSource = "kobo" | "goodreads";

const KEY = "parallax.readingSource";

/** The source Reading last showed, from this window's storage. */
export function savedReadingSource(): ReadingSource {
  try {
    return localStorage.getItem(KEY) === "goodreads" ? "goodreads" : "kobo";
  } catch {
    return "kobo";
  }
}

export function saveReadingSource(source: ReadingSource) {
  try {
    localStorage.setItem(KEY, source);
  } catch {
    // Storage can be unavailable; Reading then opens on the Kobo.
  }
}

interface Props {
  source: ReadingSource;
  onSource: (source: ReadingSource) => void;
  /** The connected Kobo; null shows the last saved stats. */
  mount: string | null;
  library: Book[];
  /** Changes when the saved Goodreads books may have. */
  goodreadsStamp: unknown;
  onGoodreadsImport: () => void;
}

/** Reading: your Kobo's own stats, or what your Goodreads shelves say. */
export function ReadingTab({
  source,
  onSource,
  mount,
  library,
  goodreadsStamp,
  onGoodreadsImport,
}: Props) {
  // Each source stays mounted once shown, so switching back to it keeps what
  // it loaded and where it was instead of flashing its loading text.
  const [shown, setShown] = useState<Set<ReadingSource>>(
    () => new Set([source]),
  );
  if (!shown.has(source)) setShown(new Set([...shown, source]));
  return (
    <div className="reading-tab">
      <div
        className="segmented reading-switch"
        role="group"
        aria-label="Reading from"
      >
        {(
          [
            ["kobo", "Kobo"],
            ["goodreads", "Goodreads"],
          ] as const
        ).map(([s, label]) => (
          <button
            key={s}
            aria-pressed={source === s}
            className={source === s ? "active" : ""}
            onClick={() => onSource(s)}
          >
            {label}
          </button>
        ))}
      </div>
      {shown.has("kobo") && (
        <div className="reading-source-panel" hidden={source !== "kobo"}>
          <ReadingView mount={mount} library={library} />
        </div>
      )}
      {shown.has("goodreads") && (
        <div className="reading-source-panel" hidden={source !== "goodreads"}>
          <GoodreadsView stamp={goodreadsStamp} onImport={onGoodreadsImport} />
        </div>
      )}
    </div>
  );
}
