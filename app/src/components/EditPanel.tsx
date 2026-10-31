import { useState } from "react";
import { coverSrc, type Book, type MetadataEdit } from "../api";
import { fromForm, toForm, type EditForm } from "../library";

interface Props {
  book: Book;
  onSave: (id: number, edit: MetadataEdit) => Promise<void>;
  onClose: () => void;
}

/** Keyed by book id + modification time in the parent, so it remounts with a
 * fresh form when the selection or the saved book changes. */
export function EditPanel({ book, onSave, onClose }: Props) {
  const [form, setForm] = useState<EditForm>(() => toForm(book));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const dirty =
    JSON.stringify(fromForm(form)) !== JSON.stringify(fromForm(toForm(book)));
  const field = (key: keyof EditForm) => ({
    value: form[key],
    onChange: (e: React.ChangeEvent<HTMLInputElement>) =>
      setForm({ ...form, [key]: e.target.value }),
  });

  const save = async (e: React.FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setError(null);
    try {
      await onSave(book.id, fromForm(form));
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  const src = coverSrc(book);
  return (
    <aside className="panel">
      <div className="panel-head">
        {src && <img className="panel-cover" src={src} alt="" />}
        <button className="icon" aria-label="Close" onClick={onClose}>
          ×
        </button>
      </div>
      <form onSubmit={save} className="form">
        <label>
          Title
          <input {...field("title")} required />
        </label>
        <label>
          <span>
            Authors <span className="hint">· comma-separated</span>
          </span>
          <input {...field("authors")} />
        </label>
        <div className="row">
          <label className="grow">
            Series
            <input {...field("series")} />
          </label>
          <label className="narrow">
            #
            <input
              {...field("seriesIndex")}
              inputMode="decimal"
              disabled={!form.series.trim()}
            />
          </label>
        </div>
        <label>
          <span>
            Tags <span className="hint">· comma-separated</span>
          </span>
          <input {...field("tags")} />
        </label>
        {error && <p className="notice notice-error">{error}</p>}
        <p className="hint">
          Changes apply to the copy sent to your Kobo. Your original file is
          never modified.
        </p>
        <div className="form-actions">
          <button
            type="button"
            className="secondary"
            disabled={!dirty || saving}
            onClick={() => setForm(toForm(book))}
          >
            Revert
          </button>
          <button
            type="submit"
            className="primary"
            disabled={!dirty || saving || !form.title.trim()}
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </form>
      <dl className="facts">
        {book.publisher && (
          <>
            <dt>Publisher</dt>
            <dd>{book.publisher}</dd>
          </>
        )}
        {book.language && (
          <>
            <dt>Language</dt>
            <dd>{book.language}</dd>
          </>
        )}
        <dt>File</dt>
        <dd title={book.original_name}>{book.original_name}</dd>
      </dl>
    </aside>
  );
}
