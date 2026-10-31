import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  coverSrc,
  resetCover,
  setCoverFromFile,
  setCoverFromUrl,
  type Book,
  type MetadataEdit,
} from "../api";
import { fromForm, toForm, type EditForm } from "../library";
import { CoverPicker } from "./CoverPicker";

interface Props {
  book: Book;
  onSave: (id: number, edit: MetadataEdit) => Promise<void>;
  /** Called with the updated book after a cover change. */
  onChanged: (book: Book) => void;
  onClose: () => void;
}

/** Keyed by book id in the parent, so selecting another book starts a fresh form. */
export function EditPanel({ book, onSave, onChanged, onClose }: Props) {
  const [form, setForm] = useState<EditForm>(() => toForm(book));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [picker, setPicker] = useState(false);
  const [coverBusy, setCoverBusy] = useState(false);

  const changeCover = async (action: () => Promise<Book>) => {
    setCoverBusy(true);
    setError(null);
    try {
      onChanged(await action());
    } catch (err) {
      setError(String(err));
    } finally {
      setCoverBusy(false);
    }
  };

  const chooseImage = async () => {
    const path = await open({
      filters: [
        { name: "Images", extensions: ["jpg", "jpeg", "png", "webp", "gif"] },
      ],
    });
    if (typeof path === "string")
      changeCover(() => setCoverFromFile(book.id, path));
  };

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
        <div className="panel-cover-wrap">
          {src ? (
            <img className="panel-cover" src={src} alt="" />
          ) : (
            <div className="panel-cover placeholder">No cover</div>
          )}
          <div className="cover-actions">
            <button
              className="link"
              disabled={coverBusy}
              onClick={() => setPicker(true)}
            >
              Find cover online…
            </button>
            <button className="link" disabled={coverBusy} onClick={chooseImage}>
              Choose image…
            </button>
            {book.custom_cover && (
              <button
                className="link"
                disabled={coverBusy}
                onClick={() => changeCover(() => resetCover(book.id))}
              >
                Use original cover
              </button>
            )}
          </div>
        </div>
        <button className="icon" aria-label="Close" onClick={onClose}>
          ×
        </button>
      </div>
      {picker && (
        <CoverPicker
          book={book}
          onPick={(url) => changeCover(() => setCoverFromUrl(book.id, url))}
          onClose={() => setPicker(false)}
        />
      )}
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
