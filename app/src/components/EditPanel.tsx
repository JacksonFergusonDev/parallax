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
import { ROLES, fromForm, toForm, type EditForm } from "../library";
import { AnnotationsDialog } from "./AnnotationsDialog";
import { CoverPicker } from "./CoverPicker";

interface Props {
  book: Book;
  onSave: (id: number, edit: MetadataEdit) => Promise<void>;
  /** Called with the updated book after a cover change. */
  onChanged: (book: Book) => void;
  /** Look up this book's series and genres online. */
  onEnhance: (id: number) => void;
  onClose: () => void;
}

/** Keyed by book id in the parent, so selecting another book starts a fresh form. */
export function EditPanel({
  book,
  onSave,
  onChanged,
  onEnhance,
  onClose,
}: Props) {
  const [form, setForm] = useState<EditForm>(() => toForm(book));
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [picker, setPicker] = useState(false);
  const [highlights, setHighlights] = useState(false);
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
  const field = (
    key: "title" | "authors" | "series" | "seriesIndex" | "tags",
  ) => ({
    value: form[key],
    onChange: (e: React.ChangeEvent<HTMLInputElement>) =>
      setForm((form) => ({ ...form, [key]: e.target.value })),
  });

  const setContributor = (
    i: number,
    change: Partial<EditForm["contributors"][0]>,
  ) =>
    setForm((form) => ({
      ...form,
      contributors: form.contributors.map((c, j) =>
        j === i ? { ...c, ...change } : c,
      ),
    }));

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
      {highlights && (
        <AnnotationsDialog book={book} onClose={() => setHighlights(false)} />
      )}
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
        <fieldset className="contributors">
          <legend>Contributors</legend>
          {form.contributors.map((c, i) => (
            <div key={i} className="contributor">
              <input
                aria-label="Name"
                placeholder="Name"
                value={c.name}
                onChange={(e) => setContributor(i, { name: e.target.value })}
              />
              <select
                aria-label="Role"
                value={c.role}
                onChange={(e) => setContributor(i, { role: e.target.value })}
              >
                {!ROLES.some(([code]) => code === c.role) && (
                  <option value={c.role}>{c.role || "Role"}</option>
                )}
                {ROLES.map(([code, label]) => (
                  <option key={code} value={code}>
                    {label}
                  </option>
                ))}
              </select>
              <button
                type="button"
                className="icon"
                aria-label={`Remove ${c.name || "contributor"}`}
                onClick={() =>
                  setForm((form) => ({
                    ...form,
                    contributors: form.contributors.filter((_, j) => j !== i),
                  }))
                }
              >
                ×
              </button>
            </div>
          ))}
          <button
            type="button"
            className="link"
            onClick={() =>
              setForm((form) => ({
                ...form,
                contributors: [...form.contributors, { name: "", role: "trl" }],
              }))
            }
          >
            Add translator, illustrator…
          </button>
        </fieldset>
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
        <button
          type="button"
          className="link find-online"
          onClick={() => onEnhance(book.id)}
        >
          Find series and genres online…
        </button>
        {(book.annotations ?? 0) > 0 && (
          <button
            type="button"
            className="link find-online"
            onClick={() => setHighlights(true)}
          >
            {`Highlights and notes (${book.annotations})…`}
          </button>
        )}
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
