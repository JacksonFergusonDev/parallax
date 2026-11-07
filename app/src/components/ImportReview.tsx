import { useCallback, useEffect, useMemo, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  coverSrc,
  dismissDuplicates,
  enhanceApply,
  enhancePropose,
  getSetting,
  onEnhanceProgress,
  removeBooks,
  setCoverFromUrl,
  setSetting,
  updateBook,
  type Book,
  type Proposal,
} from "../api";
import { libraryReviewGroups } from "../duplicates";
import {
  countChanges,
  initialSelection,
  toAccepted,
  toggleGenre,
  toggleSeries,
  type Selection,
} from "../enhance";
import { GENRES } from "../genres";
import {
  coverIds,
  findBetterCover,
  lacksGenre,
  manualIds,
  plural,
  reviewProposals,
  reviewSteps,
  STEP_NAMES,
  summaryLines,
  type CoverSearch,
  type Lookup,
  type ReviewBatch,
  type Step,
} from "../importReview";
import { fromForm, toForm } from "../library";
import { CoverPicker } from "./CoverPicker";
import { DuplicateReview, type ReviewGroup } from "./DuplicateReview";
import { ProposalChanges } from "./EnhanceReview";
import { UpscaleReview } from "./UpscaleReview";

export const REVIEW_SETTING = "review_on_add";

interface Props {
  batch: ReviewBatch;
  books: Map<number, Book>;
  /** A connected Kobo whose series and folders are used as clues. */
  mount: string | null;
  canUpscale: boolean;
  onKobo: (id: number) => boolean;
  /** Something was saved; the library should be reloaded. */
  onChanged: () => void;
  onClose: () => void;
}

type Screen = "summary" | Step | "upscale" | "done";

type CoverChoice = "keep" | "use" | "upscale";

interface CoverRow extends CoverSearch {
  choice: CoverChoice;
  /** Chosen in the picker, in place of `best`. */
  picked: string | null;
}

/** Details typed in for one book; only the fields it needs. */
interface ManualEdit {
  title?: string;
  author?: string;
  genre?: string;
}

const ORDER: Step[] = ["duplicates", "metadata", "covers"];

/**
 * Says what was found in books just added, then walks through one kind of
 * decision at a time. The books are already in the library, and each step
 * saves as it's left, so closing at any point loses nothing.
 */
export function ImportReview({
  batch,
  books,
  mount,
  canUpscale,
  onKobo,
  onChanged,
  onClose,
}: Props) {
  const { ids, review } = batch;
  const [screen, setScreen] = useState<Screen>("summary");
  const [lookup, setLookup] = useState<Lookup>({
    kind: "running",
    done: 0,
    total: 0,
    title: "",
  });
  const [selection, setSelection] = useState<Selection>(new Map());
  const [edits, setEdits] = useState<Map<number, ManualEdit>>(new Map());
  const [covers, setCovers] = useState<Map<number, CoverRow>>(new Map());
  const [picking, setPicking] = useState<number | null>(null);
  const [removed, setRemoved] = useState<Set<number>>(new Set());
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState({ details: 0, covers: 0, upscaled: 0 });
  /** Set when the duplicates step opens, so refreshes don't reshuffle it. */
  const [duplicateGroups, setDuplicateGroups] = useState<Promise<
    ReviewGroup[]
  > | null>(null);

  const searchCover = useCallback(
    async (id: number) => {
      const search = await findBetterCover(id);
      const choice: CoverChoice = search.best
        ? "use"
        : canUpscale && search.current !== null
          ? "upscale"
          : "keep";
      setCovers((c) => new Map(c).set(id, { ...search, choice, picked: null }));
    },
    [canUpscale],
  );

  // Looks everything up as soon as the window opens, so the answers are
  // there by the time they're needed. Nothing is saved without the user.
  useEffect(() => {
    let alive = true;
    const unlisten = onEnhanceProgress((p) => {
      if (alive)
        setLookup({
          kind: "running",
          done: p.index,
          total: p.total,
          title: p.title,
        });
    });
    (async () => {
      try {
        const all = await enhancePropose(ids, mount);
        if (!alive) return;
        const proposals = reviewProposals(all, review);
        setSelection(initialSelection(proposals));
        setLookup({
          kind: "done",
          proposals,
          problems: [...new Set(all.flatMap((p: Proposal) => p.problems))],
        });
      } catch (e) {
        if (alive) setLookup({ kind: "failed", message: String(e) });
      }
      unlisten.then((f) => f());
      // Covers after lookups: both spend the same services' allowances.
      for (const id of coverIds(review)) {
        if (!alive) return;
        await searchCover(id);
      }
    })();
    return () => {
      alive = false;
      unlisten.then((f) => f());
    };
  }, [ids, review, mount, searchCover]);

  const wanted = useMemo(() => coverIds(review, removed), [review, removed]);
  const searched = wanted.every((id) => covers.has(id));
  const offerable = wanted.some((id) => {
    const row = covers.get(id);
    return row && (row.best || (canUpscale && row.current !== null));
  });
  const betterCovers = searched
    ? wanted.filter((id) => covers.get(id)?.best).length
    : null;
  const steps = reviewSteps(
    batch,
    lookup,
    removed,
    offerable ? true : searched ? false : null,
  );

  const title = useCallback(
    (id: number) => books.get(id)?.title ?? `Book ${id}`,
    [books],
  );

  /** Moves past `from`, ending the review when nothing's left. */
  const advance = (from: Screen, changes = done) => {
    const position = ORDER.indexOf(from as Step);
    const next = steps.find((s) => ORDER.indexOf(s) > position);
    if (next) setScreen(next);
    else if (
      changes.details + changes.covers + changes.upscaled > 0 ||
      removed.size > 0
    )
      setScreen("done");
    else onClose();
  };

  const remove = async (target: number[]) => {
    await removeBooks(target);
    setRemoved((r) => new Set([...r, ...target]));
    onChanged();
  };

  const removeWarned = async (target: number[]) => {
    const names = target.map(title).join(", ");
    if (await ask(`Remove ${names} from your library?`, { kind: "warning" }))
      await remove(target).catch((e) => setError(String(e)));
  };

  const proposals =
    lookup.kind === "done"
      ? reviewProposals(lookup.proposals, review, removed)
      : [];
  const manual = manualIds(review, lookup, removed);
  const position = steps.indexOf(screen as Step) + 1;
  const stepLabel = `Step ${position} of ${steps.length}`;

  const saveDetails = async () => {
    setSaving(true);
    setError(null);
    try {
      const accepted = toAccepted(proposals, selection);
      const saved = new Map(books);
      const updated = accepted.length ? await enhanceApply(accepted) : [];
      for (const b of updated) saved.set(b.id, b);
      const changed = new Set(updated.map((b) => b.id));
      const renamed: number[] = [];
      for (const [id, edit] of edits) {
        const book = saved.get(id);
        if (!book) continue;
        const form = toForm(book);
        const typed = edit.genre?.trim();
        const next = {
          ...form,
          title: edit.title?.trim() || form.title,
          authors: edit.author?.trim() || form.authors,
          genres: typed ? [...form.genres, typed] : form.genres,
        };
        const same =
          next.title === form.title && next.authors === form.authors && !typed;
        if (same) continue;
        await updateBook(id, fromForm(next));
        changed.add(id);
        if (next.title !== form.title || next.authors !== form.authors)
          renamed.push(id);
      }
      const changes = { ...done, details: changed.size };
      setDone(changes);
      if (changed.size) onChanged();
      // Covers found under the old title or author may be wrong, or missing.
      const again = renamed.filter((id) => wanted.includes(id));
      setCovers((c) => {
        const kept = new Map(c);
        for (const id of again) kept.delete(id);
        return kept;
      });
      void (async () => {
        for (const id of again) await searchCover(id);
      })();
      advance("metadata", changes);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const saveCovers = async () => {
    setSaving(true);
    setError(null);
    try {
      let replaced = 0;
      for (const id of wanted) {
        const row = covers.get(id);
        const url = row?.picked ?? row?.best?.candidate.url;
        if (row?.choice !== "use" || !url) continue;
        await setCoverFromUrl(id, url);
        replaced++;
      }
      const changes = { ...done, covers: replaced };
      setDone(changes);
      if (replaced) onChanged();
      const upscale = wanted.filter(
        (id) => covers.get(id)?.choice === "upscale",
      );
      if (upscale.length) setScreen("upscale");
      else advance("covers", changes);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  const start = () => {
    if (steps[0] === "duplicates")
      setDuplicateGroups(
        Promise.resolve(
          libraryReviewGroups(review.duplicates, books, onKobo, coverSrc),
        ),
      );
    setScreen(steps[0]);
  };

  if (screen === "duplicates" && duplicateGroups)
    return (
      <DuplicateReview
        place="library"
        step={stepLabel}
        groups={duplicateGroups}
        removeWarning="They're removed from your library; copies on your Kobo stay."
        remove={(target) => remove(target.map(Number))}
        dismiss={(key) => dismissDuplicates(key, null)}
        onChanged={onChanged}
        onClose={() => advance("duplicates")}
      />
    );

  if (screen === "upscale")
    return (
      <UpscaleReview
        books={wanted.flatMap((id) => {
          const book = books.get(id);
          return book && covers.get(id)?.choice === "upscale" ? [book] : [];
        })}
        onChanged={() => {
          setDone((d) => ({ ...d, upscaled: d.upscaled + 1 }));
          onChanged();
        }}
        onClose={() => setScreen("done")}
      />
    );

  const added = ids.length - removed.size;
  return (
    <div className="modal-backdrop">
      <div
        className="modal enhance import-review"
        role="dialog"
        aria-label="Review new books"
      >
        <div className="modal-head">
          <div>
            {position > 0 && (
              <p className="review-step">
                {stepLabel} · {STEP_NAMES[screen as Step]}
              </p>
            )}
            <h2>
              {screen === "summary"
                ? added > 0
                  ? `${plural(added, "book")} added`
                  : "Nothing was added"
                : screen === "metadata"
                  ? "Check the book details"
                  : screen === "covers"
                    ? "Choose better covers"
                    : "All done"}
            </h2>
            <p className="muted">
              {screen === "summary"
                ? added > 0
                  ? "They're in your library. Here's what could be better."
                  : "Here's what went wrong."
                : screen === "metadata"
                  ? "Ticked items are sure matches. Nothing is saved until you continue."
                  : screen === "covers"
                    ? "Each book starts on the best option we found."
                    : "Your books are ready."}
            </p>
          </div>
          <button className="icon" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>

        {error && <p className="notice notice-error">{error}</p>}

        {screen === "summary" && (
          <>
            <ul className="review-summary">
              {summaryLines(batch, lookup, removed, betterCovers, title).map(
                (line) => (
                  <li key={line.key} className={line.warning ? "warning" : ""}>
                    <div>
                      <div className="review-line">{line.text}</div>
                      <div className="muted">{line.advice}</div>
                    </div>
                    {line.removable && (
                      <button
                        className="link"
                        onClick={() => removeWarned(line.removable ?? [])}
                      >
                        Remove
                      </button>
                    )}
                  </li>
                ),
              )}
            </ul>
            <div className="form-actions review-actions">
              <ReviewOnAdd />
              <span className="spacer" />
              {steps.length > 0 ? (
                <>
                  <button className="secondary" onClick={onClose}>
                    Keep as is
                  </button>
                  <button className="primary" autoFocus onClick={start}>
                    Review
                  </button>
                </>
              ) : (
                <button className="primary" autoFocus onClick={onClose}>
                  Done
                </button>
              )}
            </div>
          </>
        )}

        {screen === "metadata" && (
          <>
            {lookup.kind === "running" && (
              <>
                <p className="muted">
                  {lookup.total
                    ? `Looking up ${lookup.done} of ${lookup.total}: ${lookup.title}`
                    : "Looking up your books…"}
                </p>
                <progress
                  className="enhance-progress"
                  value={lookup.done}
                  max={lookup.total || 1}
                />
              </>
            )}
            {lookup.kind !== "running" &&
              proposals.length + manual.length === 0 && (
                <p className="muted modal-empty">
                  Nothing to fill in for these books.
                </p>
              )}
            <ul className="enhance-list">
              {[
                ...new Set([...proposals.map((p) => p.book_id), ...manual]),
              ].map((id) => {
                const book = books.get(id);
                const proposal = proposals.find((p) => p.book_id === id);
                const src = book ? coverSrc(book) : null;
                const edit = edits.get(id) ?? {};
                const change = (patch: ManualEdit) =>
                  setEdits((e) => new Map(e).set(id, { ...edit, ...patch }));
                const has = (issue: string) =>
                  review.findings.some(
                    (f) => f.book_id === id && f.issue === issue,
                  );
                return (
                  <li key={id}>
                    <span className="thumb">
                      {src ? <img src={src} alt="" loading="lazy" /> : null}
                    </span>
                    <div className="enhance-book">
                      <div className="row-title">{title(id)}</div>
                      <div className="row-sub">{book?.authors.join(", ")}</div>
                      {proposal && (
                        <ProposalChanges
                          proposal={proposal}
                          choice={selection.get(id)}
                          onToggleSeries={() =>
                            setSelection((s) => toggleSeries(s, id))
                          }
                          onToggleGenre={(g) =>
                            setSelection((s) => toggleGenre(s, id, g))
                          }
                        />
                      )}
                      {has("title_from_file") && (
                        <label className="review-field">
                          <span className="enhance-field">Title</span>
                          <input
                            value={edit.title ?? book?.title ?? ""}
                            onChange={(e) => change({ title: e.target.value })}
                          />
                        </label>
                      )}
                      {has("no_author") && (
                        <label className="review-field">
                          <span className="enhance-field">Author</span>
                          <input
                            value={edit.author ?? ""}
                            placeholder="Author's name"
                            onChange={(e) => change({ author: e.target.value })}
                          />
                        </label>
                      )}
                      {lookup.kind !== "running" &&
                        lacksGenre(review, removed).includes(id) &&
                        !proposal?.genres.length && (
                          <label className="review-field">
                            <span className="enhance-field">Genre</span>
                            <input
                              list="review-genres"
                              value={edit.genre ?? ""}
                              placeholder="Nothing found online; add one?"
                              onChange={(e) =>
                                change({ genre: e.target.value })
                              }
                            />
                          </label>
                        )}
                    </div>
                  </li>
                );
              })}
            </ul>
            <datalist id="review-genres">
              {GENRES.map((g) => (
                <option key={g} value={g} />
              ))}
            </datalist>
            {lookup.kind === "failed" && (
              <p className="muted enhance-problems">
                Couldn't look books up: {lookup.message}
              </p>
            )}
            {lookup.kind === "done" && lookup.problems.length > 0 && (
              <p className="muted enhance-problems">
                Some sources didn't answer: {lookup.problems.join("; ")}
              </p>
            )}
            <div className="form-actions">
              <button
                className="secondary"
                disabled={saving}
                onClick={() => advance("metadata")}
              >
                Skip this step
              </button>
              <button
                className="primary"
                autoFocus
                disabled={saving || lookup.kind === "running"}
                onClick={saveDetails}
              >
                {saving
                  ? "Saving…"
                  : countChanges(selection) + edits.size > 0
                    ? "Save and continue"
                    : "Continue"}
              </button>
            </div>
          </>
        )}

        {screen === "covers" && (
          <>
            {searched && !offerable && (
              <p className="muted modal-empty">No better covers were found.</p>
            )}
            <ul className="enhance-list review-covers">
              {wanted.map((id) => {
                const book = books.get(id);
                const row = covers.get(id);
                const src = book ? coverSrc(book) : null;
                const offered =
                  row?.picked ?? row?.best?.candidate.preview_url ?? null;
                const set = (patch: Partial<CoverRow>) =>
                  row &&
                  setCovers((c) => new Map(c).set(id, { ...row, ...patch }));
                return (
                  <li key={id}>
                    <span className="thumb review-thumb">
                      {src ? <img src={src} alt="" loading="lazy" /> : null}
                    </span>
                    <div className="enhance-book">
                      <div className="row-title">{title(id)}</div>
                      <div className="row-sub">
                        {row?.current
                          ? `Current cover is ${row.current} px tall`
                          : "No cover yet"}
                      </div>
                      {!row ? (
                        <div className="muted">Searching…</div>
                      ) : (
                        <div className="review-cover-choices">
                          {(row.best || row.picked) && (
                            <label>
                              <input
                                type="radio"
                                name={`cover-${id}`}
                                checked={row.choice === "use"}
                                onChange={() => set({ choice: "use" })}
                              />
                              {row.picked
                                ? "Use the cover you chose"
                                : `Use the ${row.best?.candidate.source} cover (${row.best?.width}×${row.best?.height})`}
                            </label>
                          )}
                          {canUpscale && row.current !== null && (
                            <label>
                              <input
                                type="radio"
                                name={`cover-${id}`}
                                checked={row.choice === "upscale"}
                                onChange={() => set({ choice: "upscale" })}
                              />
                              Upscale the current cover
                            </label>
                          )}
                          <label>
                            <input
                              type="radio"
                              name={`cover-${id}`}
                              checked={row.choice === "keep"}
                              onChange={() => set({ choice: "keep" })}
                            />
                            Keep it as it is
                          </label>
                          <button
                            className="link"
                            onClick={() => setPicking(id)}
                          >
                            See all covers…
                          </button>
                        </div>
                      )}
                    </div>
                    {offered && row?.choice === "use" && (
                      <span className="thumb review-thumb">
                        <img src={offered} alt="" loading="lazy" />
                      </span>
                    )}
                  </li>
                );
              })}
            </ul>
            <div className="form-actions">
              <button
                className="secondary"
                disabled={saving}
                onClick={() => advance("covers")}
              >
                Skip this step
              </button>
              <button
                className="primary"
                autoFocus
                disabled={saving}
                onClick={saveCovers}
              >
                {saving ? "Saving…" : "Save and continue"}
              </button>
            </div>
          </>
        )}

        {screen === "done" && (
          <>
            <ul className="review-summary">
              {done.details > 0 && (
                <li>Updated the details of {plural(done.details, "book")}</li>
              )}
              {done.covers > 0 && (
                <li>Replaced {plural(done.covers, "cover")}</li>
              )}
              {done.upscaled > 0 && (
                <li>Upscaled {plural(done.upscaled, "cover")}</li>
              )}
              {removed.size > 0 && (
                <li>Removed {plural(removed.size, "book")}</li>
              )}
              {done.details + done.covers + done.upscaled + removed.size ===
                0 && <li>Nothing was changed.</li>}
            </ul>
            <div className="form-actions">
              <button className="primary" autoFocus onClick={onClose}>
                Done
              </button>
            </div>
          </>
        )}
      </div>
      {picking !== null && books.get(picking) && (
        <CoverPicker
          book={books.get(picking) as Book}
          onPick={async (url) => {
            const row = covers.get(picking);
            if (row)
              setCovers((c) =>
                new Map(c).set(picking, { ...row, picked: url, choice: "use" }),
              );
          }}
          onClose={() => setPicking(null)}
        />
      )}
    </div>
  );
}

/** Turns the review after adding books on or off. */
export function ReviewOnAdd() {
  const [on, setOn] = useState<boolean | null>(null);

  useEffect(() => {
    let alive = true;
    getSetting(REVIEW_SETTING)
      .then((value) => alive && setOn(value !== "off"))
      .catch(() => alive && setOn(true));
    return () => {
      alive = false;
    };
  }, []);

  return (
    <label className="review-toggle">
      <input
        type="checkbox"
        disabled={on === null}
        checked={on ?? true}
        onChange={(e) => {
          const next = e.target.checked;
          setOn(next);
          void setSetting(REVIEW_SETTING, next ? null : "off");
        }}
      />
      Review books when added
    </label>
  );
}
