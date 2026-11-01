import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import {
  annotationsExport,
  annotationsList,
  type Annotation,
  type Book,
} from "../api";

interface Props {
  book: Book;
  onClose: () => void;
}

/** Each annotation with the chapter heading to show before it, if any. */
function withHeadings(items: Annotation[]) {
  return items.map((a, i) => {
    const previous = items
      .slice(0, i)
      .reverse()
      .find((b) => b.chapter)?.chapter;
    return {
      a,
      heading: a.chapter && a.chapter !== previous ? a.chapter : null,
    };
  });
}

/** A file name without characters macOS or other systems reject. */
const safeName = (s: string) => s.replace(/[/\\:*?"<>|]/g, "_").trim();

/** A book's highlights and notes from the Kobo, with export. */
export function AnnotationsDialog({ book, onClose }: Props) {
  const [items, setItems] = useState<Annotation[] | null>(null);
  const [message, setMessage] = useState<{
    error: boolean;
    text: string;
  } | null>(null);

  useEffect(() => {
    let alive = true;
    annotationsList(book.id)
      .then((a) => alive && setItems(a))
      .catch((e) => alive && setMessage({ error: true, text: String(e) }));
    return () => {
      alive = false;
    };
  }, [book.id]);

  const exportAs = async (format: "markdown" | "json") => {
    const ext = format === "json" ? "json" : "md";
    const path = await save({
      defaultPath: `${safeName(book.title)} highlights.${ext}`,
      filters: [
        { name: format === "json" ? "JSON" : "Markdown", extensions: [ext] },
      ],
    });
    if (!path) return;
    try {
      await annotationsExport(book.id, format, path);
      setMessage({ error: false, text: `Saved to ${path}` });
    } catch (e) {
      setMessage({ error: true, text: String(e) });
    }
  };

  const shown = (items ?? []).filter((a) => a.text || a.note);
  const bookmarks = (items ?? []).length - shown.length;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal annotations" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <div>
            <h2>Highlights and notes</h2>
            <p className="muted">
              {book.title}
              {items &&
                ` · ${shown.length} ${shown.length === 1 ? "highlight" : "highlights"}` +
                  (bookmarks
                    ? ` · ${bookmarks} bookmarked ${bookmarks === 1 ? "page" : "pages"}`
                    : "")}
            </p>
          </div>
        </div>
        <div className="annotation-list">
          {items?.length === 0 && (
            <p className="muted">
              Nothing yet. Highlights made on your Kobo appear here after you
              connect it.
            </p>
          )}
          {withHeadings(shown).map(({ a, heading }) => {
            return (
              <div key={`${a.device_serial}-${a.id}`}>
                {heading && <h3>{heading}</h3>}
                <figure className={a.on_device ? "" : "removed"}>
                  {a.text && <blockquote>{a.text}</blockquote>}
                  {a.note && <figcaption>{a.note}</figcaption>}
                  {!a.on_device && (
                    <p className="muted">Removed from the Kobo; kept here.</p>
                  )}
                </figure>
              </div>
            );
          })}
        </div>
        {message && (
          <p className={`notice notice-${message.error ? "error" : "info"}`}>
            {message.text}
          </p>
        )}
        <div className="form-actions">
          <button
            className="secondary"
            disabled={!shown.length}
            onClick={() => exportAs("markdown")}
          >
            Export Markdown…
          </button>
          <button
            className="secondary"
            disabled={!items?.length}
            onClick={() => exportAs("json")}
          >
            Export JSON…
          </button>
          <span className="spacer" />
          <button className="primary" onClick={onClose} autoFocus>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
