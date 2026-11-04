import { useEffect, useState } from "react";
import { genresApply, genresPlan, type TidyPlan } from "../api";

interface Props {
  /** The connected, writable Kobo, whose collections are tidied too. */
  mount: string | null;
  onDone: () => void;
  onClose: () => void;
}

const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
const key = (from: string, to: string) => `${from}\u0000${to}`;

/** Renames genre tags and Kobo collections to the standard genre names. */
export function TidyGenres({ mount, onDone, onClose }: Props) {
  const [plan, setPlan] = useState<TidyPlan | null>(null);
  const [tags, setTags] = useState<Set<string>>(new Set());
  const [collections, setCollections] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [done, setDone] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    genresPlan(mount)
      .then((p) => {
        if (!alive) return;
        setPlan(p);
        // Variant spellings are chosen; tags that only mention a genre aren't.
        setTags(
          new Set(p.tags.filter((t) => t.exact).map((t) => key(t.from, t.to))),
        );
        setCollections(new Set(p.collections.map((c) => key(c.from, c.to))));
      })
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [mount]);

  const toggle =
    (set: Set<string>, update: (s: Set<string>) => void, k: string) => () => {
      const next = new Set(set);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      update(next);
    };

  const apply = async () => {
    if (!plan) return;
    setBusy(true);
    setError(null);
    const pairs = (set: Set<string>) =>
      [...set].map((k) => k.split("\u0000") as [string, string]);
    try {
      const result = await genresApply(mount, {
        tags: pairs(tags),
        collections: pairs(collections),
      });
      setDone(
        [
          result.books && `${plural(result.books, "book")} retagged`,
          result.collections &&
            `${plural(result.collections, "collection")} on your Kobo renamed or merged`,
        ]
          .filter(Boolean)
          .join(", ") + ".",
      );
      onDone();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const nothing =
    plan && plan.tags.length === 0 && plan.collections.length === 0;
  const chosen = tags.size + collections.size;

  return (
    <div className="modal-backdrop" onClick={busy ? undefined : onClose}>
      <div
        className="modal tidy-genres"
        role="dialog"
        aria-label="Tidy genres"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <div>
            <h2>Tidy genres</h2>
            <p className="muted">
              Gives every genre one standard name, so “sci-fi”, “SciFi” and
              “Science fiction” all become Science Fiction and collections made
              from them don't split.
              {mount
                ? " Your Kobo's genre collections are renamed too (backed up first); its folders are left as they are."
                : " Connect your Kobo to tidy its collections too."}
            </p>
          </div>
        </div>
        {error && <p className="notice notice-error wrap">{error}</p>}
        {done ? (
          <p>{done}</p>
        ) : !plan ? (
          !error && <p className="muted">Looking at your genres…</p>
        ) : nothing ? (
          <p>Every genre already uses its standard name.</p>
        ) : (
          <div className="tidy-lists">
            {plan.tags.length > 0 && (
              <section>
                <h3>Tags in your library</h3>
                <ul>
                  {plan.tags.map((t) => {
                    const k = key(t.from, t.to);
                    return (
                      <li key={k}>
                        <label>
                          <input
                            type="checkbox"
                            checked={tags.has(k)}
                            onChange={toggle(tags, setTags, k)}
                          />
                          <span>
                            “{t.from}” → <strong>{t.to}</strong>
                          </span>
                          <span className="muted">
                            {plural(t.book_ids.length, "book")}
                            {!t.exact && " · mentions this genre"}
                          </span>
                        </label>
                      </li>
                    );
                  })}
                </ul>
              </section>
            )}
            {plan.collections.length > 0 && (
              <section>
                <h3>Collections on your Kobo</h3>
                <ul>
                  {plan.collections.map((c) => {
                    const k = key(c.from, c.to);
                    return (
                      <li key={k}>
                        <label>
                          <input
                            type="checkbox"
                            checked={collections.has(k)}
                            onChange={toggle(collections, setCollections, k)}
                          />
                          <span>
                            “{c.from}” → <strong>{c.to}</strong>
                          </span>
                          <span className="muted">
                            {plural(c.books, "book")}
                            {c.merge && " · merged into it"}
                          </span>
                        </label>
                      </li>
                    );
                  })}
                </ul>
              </section>
            )}
          </div>
        )}
        <div className="form-actions">
          <span className="spacer" />
          {done || nothing ? (
            <button className="primary" onClick={onClose} autoFocus>
              Done
            </button>
          ) : (
            <>
              <button className="secondary" onClick={onClose} disabled={busy}>
                Cancel
              </button>
              <button
                className="primary"
                onClick={apply}
                disabled={!plan || chosen === 0 || busy}
              >
                {busy ? "Tidying…" : `Rename ${plural(chosen, "genre")}`}
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
