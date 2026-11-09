import { GeneratedCover } from "./GeneratedCover";
import { Fragment, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  coverSrc,
  comicSpreads,
  setComicKind,
  openReader,
  resetCover,
  setCoverFromFile,
  setCoverFromUrl,
  type Book,
  type CoverInfo,
  type MetadataEdit,
} from "../api";
import { GENRES, genreOf } from "../genres";
import { shortDate } from "../device";
import { formatSize } from "../duplicates";
import {
  ROLES,
  fromForm,
  isbnOf,
  languageLabel,
  pagesLabel,
  plainDescription,
  publishedLabel,
  toForm,
  type EditForm,
} from "../library";
import { AnnotationsDialog } from "./AnnotationsDialog";
import { CoverPicker } from "./CoverPicker";
import { UpscaleReview } from "./UpscaleReview";

interface Props {
  book: Book;
  /** The cover's size, known before the panel appears so nothing shifts. */
  cover: CoverInfo | null;
  /** Whether this build has the upscaling model. */
  canUpscale: boolean;
  onSave: (id: number, edit: MetadataEdit) => Promise<void>;
  /** Called with the updated book after a cover change. */
  onChanged: (book: Book) => void;
  /** Look up this book's series and genres online. */
  onEnhance: (id: number) => void;
  onClose: () => void;
}

/** What a cover that isn't excellent will look like, and what would help. */
function coverAdvice(cover: CoverInfo, canUpscale: boolean) {
  const look =
    cover.quality === "low"
      ? "Will look pixellated on an e-reader."
      : "May look slightly soft on an e-reader.";
  return canUpscale
    ? `${look} Upscaling or a sharper cover would help.`
    : `${look} A sharper cover would help.`;
}

/** Longer descriptions start collapsed. */
const LONG_DESCRIPTION = 320;

function Description({ html }: { html: string }) {
  const [expanded, setExpanded] = useState(false);
  const text = plainDescription(html);
  if (!text) return null;
  const long = text.length > LONG_DESCRIPTION;
  return (
    <section className="description">
      <p className={long && !expanded ? "clamped" : undefined}>{text}</p>
      {long && (
        <button className="link" onClick={() => setExpanded(!expanded)}>
          {expanded ? "Show less" : "Show more"}
        </button>
      )}
    </section>
  );
}

/** The book's details that aren't edited here; missing ones are left out. */
function Facts({ book }: { book: Book }) {
  const comic = book.format === "cbz";
  const [spreads, setSpreads] = useState<number | null>(null);
  useEffect(() => {
    if (!comic) return;
    let active = true;
    comicSpreads(book.id).then(
      (count) => {
        if (active) setSpreads(count);
      },
      () => {},
    );
    return () => {
      active = false;
    };
  }, [book.id, comic]);
  const facts: [string, string | null | undefined, string?][] = [
    ["Published", book.published && publishedLabel(book.published)],
    ["Publisher", book.publisher],
    [
      "Pages",
      pagesLabel(book),
      comic
        ? "Pages in the archive"
        : book.pages_estimated
          ? "Estimated from the word count"
          : "From the print edition",
    ],
    ["Words", comic ? null : book.words?.toLocaleString()],
    ["Spreads", spreads?.toLocaleString()],
    [
      "Direction",
      comic
        ? book.direction === "rtl"
          ? "Right to left"
          : "Left to right"
        : null,
    ],
    ["Language", book.language && languageLabel(book.language)],
    ["ISBN", isbnOf(book)],
    ["Last read", book.reading?.last_read && shortDate(book.reading.last_read)],
    ["Added", shortDate(book.added_at)],
    ["Size", formatSize(book.file_size)],
    ["File", book.original_name, book.original_name],
  ];
  return (
    <dl className="facts">
      {facts.map(
        ([label, value, title]) =>
          value && (
            <Fragment key={label}>
              <dt>{label}</dt>
              <dd title={title}>{value}</dd>
            </Fragment>
          ),
      )}
    </dl>
  );
}

/** Keyed by book id in the parent, so selecting another book starts a fresh form. */
export function EditPanel({
  book,
  cover,
  canUpscale,
  onSave,
  onChanged,
  onEnhance,
  onClose,
}: Props) {
  const [form, setForm] = useState<EditForm>(() => toForm(book));
  const [changingKind, setChangingKind] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [picker, setPicker] = useState(false);
  const [highlights, setHighlights] = useState(false);
  const [coverBusy, setCoverBusy] = useState(false);
  const [upscaling, setUpscaling] = useState(false);

  // Only offered where it can help: the upscaler has nothing to do for an
  // excellent cover, and doesn't enlarge its own output a second time.
  const offerUpscale =
    canUpscale &&
    !!cover &&
    cover.quality !== "excellent" &&
    !book.cover_upscaled;

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

  const changeKind = async (
    kind: Book["kind"],
    direction: Book["direction"],
  ) => {
    setChangingKind(true);
    setError(null);
    try {
      onChanged(await setComicKind(book.id, kind, direction));
    } catch (err) {
      setError(String(err));
    } finally {
      setChangingKind(false);
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
            <div className="panel-cover placeholder">
              <GeneratedCover title={book.title} author={book.authors[0]} />
            </div>
          )}
          <div className="cover-actions">
            <button
              className="primary"
              onClick={() => {
                setError(null);
                openReader(book.id).catch((err) => setError(String(err)));
              }}
            >
              Read
            </button>
            {cover && (
              <div className="cover-size">
                <span>
                  {cover.width}×{cover.height}{" "}
                  <span className={`cover-quality ${cover.quality}`}>
                    ({cover.quality})
                  </span>
                </span>
                {cover.quality !== "excellent" && (
                  <p className="hint">{coverAdvice(cover, offerUpscale)}</p>
                )}
              </div>
            )}
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
            {offerUpscale && (
              <button
                className="link"
                disabled={coverBusy}
                onClick={() => setUpscaling(true)}
              >
                Upscale cover…
              </button>
            )}
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
      {upscaling && (
        <UpscaleReview
          books={[book]}
          onChanged={onChanged}
          onClose={() => setUpscaling(false)}
        />
      )}
      {picker && (
        <CoverPicker
          book={book}
          onPick={(url) => changeCover(() => setCoverFromUrl(book.id, url))}
          onClose={() => setPicker(false)}
        />
      )}
      <form onSubmit={save} className="form">
        {book.format === "cbz" && (
          <div className="row">
            <label className="grow">
              Kind
              <select
                value={book.kind}
                disabled={changingKind || saving}
                onChange={(e) =>
                  void changeKind(
                    e.target.value as Book["kind"],
                    book.direction,
                  )
                }
              >
                <option value="comic">Comic</option>
                <option value="manga">Manga</option>
              </select>
            </label>
            <label className="grow">
              Direction
              <select
                value={book.direction}
                disabled={changingKind || saving}
                onChange={(e) =>
                  void changeKind(
                    book.kind,
                    e.target.value as Book["direction"],
                  )
                }
              >
                <option value="ltr">Left to right</option>
                <option value="rtl">Right to left</option>
              </select>
            </label>
          </div>
        )}
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
          <legend>
            {book.format === "cbz" ? "Art and other credits" : "Contributors"}
          </legend>
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
        <div className="genre-field">
          <span>Genres</span>
          <div className="genre-chips">
            {form.genres.map((g) => (
              <span key={g} className="chip">
                {g}
                <button
                  type="button"
                  className="chip-remove"
                  aria-label={`Remove ${g}`}
                  onClick={() =>
                    setForm((form) => ({
                      ...form,
                      genres: form.genres.filter((x) => x !== g),
                    }))
                  }
                >
                  ×
                </button>
              </span>
            ))}
            <select
              className="genre-add"
              aria-label="Add a genre"
              value=""
              onChange={(e) => {
                const g = e.target.value;
                if (g)
                  setForm((form) => ({ ...form, genres: [...form.genres, g] }));
              }}
            >
              <option value="">
                {form.genres.length ? "Add…" : "Add a genre…"}
              </option>
              {GENRES.filter((g) => !form.genres.includes(g)).map((g) => (
                <option key={g} value={g}>
                  {g}
                </option>
              ))}
            </select>
          </div>
        </div>
        <label>
          <span>
            Tags <span className="hint">· comma-separated</span>
          </span>
          <input
            {...field("tags")}
            onBlur={() =>
              // A genre typed as a tag moves to Genres.
              setForm((form) => {
                const typed = form.tags.split(",").map((t) => t.trim());
                const moved = typed
                  .map((t) => genreOf(t))
                  .filter((g): g is string => !!g && !form.genres.includes(g));
                if (!moved.length) return form;
                return {
                  ...form,
                  genres: [...form.genres, ...new Set(moved)],
                  tags: typed.filter((t) => t && !genreOf(t)).join(", "),
                };
              })
            }
          />
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
            disabled={!dirty || saving || changingKind || !form.title.trim()}
          >
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </form>
      {book.description && <Description html={book.description} />}
      <Facts book={book} />
    </aside>
  );
}
