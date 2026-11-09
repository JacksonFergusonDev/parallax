import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
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
  type SendOutcome,
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
  initialSend,
  lacksGenre,
  manualIds,
  onlySending,
  plural,
  reviewProposals,
  reviewSteps,
  sendableIds,
  STEP_NAMES,
  summaryLines,
  type CoverSearch,
  type Lookup,
  type ReviewBatch,
  type Step,
} from "../importReview";
import { fromForm, sendStatusLabel, toForm, type SendStatus } from "../library";
import { CoverPicker } from "./CoverPicker";
import { DuplicateReview, type ReviewGroup } from "./DuplicateReview";
import { ProposalChanges } from "./EnhanceReview";
import { GeneratedCover } from "./GeneratedCover";
import { UpscaleReview } from "./UpscaleReview";

export const REVIEW_SETTING = "review_on_add";

interface Props {
  batch: ReviewBatch;
  books: Map<number, Book>;
  /** A connected Kobo whose series and folders are used as clues. */
  mount: string | null;
  canUpscale: boolean;
  onKobo: (id: number) => boolean;
  /** A connected Kobo that books can be sent to. */
  kobo: {
    name: string;
    send: (ids: number[]) => Promise<SendOutcome[] | null>;
  } | null;
  /** Progress of books being sent, by book. */
  statuses: Record<number, SendStatus>;
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

const ORDER: Step[] = ["duplicates", "metadata", "covers", "send"];

/** A book's cover, or the stand-in the library shows without one. */
function CoverImage({ book, src }: { book?: Book; src?: string | null }) {
  const shown = src ?? (book ? coverSrc(book) : null);
  return shown ? (
    <img src={shown} alt="" loading="lazy" draggable={false} />
  ) : (
    <GeneratedCover title={book?.title ?? ""} size="mini" />
  );
}

/** Where the review has got to, by name. */
function StepTracker({ steps, current }: { steps: Step[]; current: Step }) {
  const position = steps.indexOf(current);
  return (
    <ol className="review-tracker" aria-label="Review steps">
      {steps.map((step, i) => (
        <li
          key={step}
          className={
            i === position ? "current" : i < position ? "finished" : ""
          }
          aria-current={i === position ? "step" : undefined}
        >
          <span className="review-tracker-number">
            {i < position ? "✓" : i + 1}
          </span>
          {STEP_NAMES[step]}
        </li>
      ))}
    </ol>
  );
}

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
  kobo,
  statuses,
  onChanged,
  onClose,
}: Props) {
  const { ids, review } = batch;
  const sendOnly = onlySending(batch);
  const [screen, setScreen] = useState<Screen>(sendOnly ? "send" : "summary");
  const [lookup, setLookup] = useState<Lookup>(
    sendOnly
      ? { kind: "done", proposals: [], problems: [] }
      : { kind: "running", done: 0, total: 0, title: "" },
  );
  const [selection, setSelection] = useState<Selection>(new Map());
  const [edits, setEdits] = useState<Map<number, ManualEdit>>(new Map());
  const [covers, setCovers] = useState<Map<number, CoverRow>>(new Map());
  const [picking, setPicking] = useState<number | null>(null);
  const [removed, setRemoved] = useState<Set<number>>(new Set());
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState({
    details: 0,
    covers: 0,
    upscaled: 0,
    sent: 0,
  });
  const [unsent, setUnsent] = useState<string[]>([]);
  const [toSend, setToSend] = useState(() =>
    initialSend(sendableIds(batch, new Set(), onKobo), review),
  );
  const [sending, setSending] = useState(false);
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
    if (sendOnly) return;
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
  }, [ids, review, mount, searchCover, sendOnly]);

  const wanted = useMemo(() => coverIds(review, removed), [review, removed]);
  const searched = wanted.every((id) => covers.has(id));
  const offerable = wanted.some((id) => {
    const row = covers.get(id);
    return row && (row.best || (canUpscale && row.current !== null));
  });
  const betterCovers = searched
    ? wanted.filter((id) => covers.get(id)?.best).length
    : null;
  const sendable = sendableIds(batch, removed, onKobo);
  // Books stay listed once sent, when they've become "on the Kobo".
  const [sendList] = useState(() => sendable);
  const steps = reviewSteps(
    batch,
    lookup,
    removed,
    offerable ? true : searched ? false : null,
    kobo !== null && sendable.length > 0,
  );
  const fixes = steps.filter((s) => s !== "send");

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
      changes.details + changes.covers + changes.upscaled + changes.sent > 0 ||
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
  const tracker = steps.includes(screen as Step) ? (
    <StepTracker steps={steps} current={screen as Step} />
  ) : null;

  const sendBooks = async () => {
    if (!kobo) return;
    setSending(true);
    setError(null);
    try {
      const outcomes = await kobo.send(sendList.filter((id) => toSend.has(id)));
      if (!outcomes) return;
      const failed = outcomes.filter((o) => "Err" in o.result);
      const changes = { ...done, sent: outcomes.length - failed.length };
      setDone(changes);
      setUnsent(
        failed.map((o) =>
          o.book_id === null
            ? o.source
            : (books.get(o.book_id)?.title ?? o.source),
        ),
      );
      setScreen("done");
    } catch (e) {
      setError(String(e));
    } finally {
      setSending(false);
    }
  };

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
        step={tracker}
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
        onClose={() => advance("covers")}
        step={<StepTracker steps={steps} current="covers" />}
      />
    );

  const added = ids.length - removed.size;
  return (
    <div className="modal-backdrop">
      <div
        className="modal enhance import-review review-large"
        role="dialog"
        aria-label="Review new books"
      >
        {tracker}
        <div className="modal-head">
          <div>
            <h2>
              {screen === "summary"
                ? added > 0
                  ? `${plural(added, "book")} added`
                  : "Nothing was added"
                : screen === "metadata"
                  ? "Check the book details"
                  : screen === "covers"
                    ? "Choose your covers"
                    : screen === "send"
                      ? "Send to your Kobo?"
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
                    ? "Click the cover you want for each book. The best one we found is already selected."
                    : screen === "send"
                      ? kobo
                        ? `Your ${kobo.name} is connected. Untick anything you don't want on it.`
                        : "Your Kobo isn't connected any more."
                      : done.sent > 0
                        ? "Eject your Kobo and unplug it to import the new books."
                        : "Your books are ready."}
            </p>
          </div>
          <button
            className="icon"
            aria-label="Close"
            disabled={sending}
            onClick={onClose}
          >
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
              {fixes.length > 0 ? (
                <>
                  <button
                    className="secondary"
                    onClick={() =>
                      steps.includes("send") ? setScreen("send") : onClose()
                    }
                  >
                    Keep as is
                  </button>
                  <button className="primary" autoFocus onClick={start}>
                    Review
                  </button>
                </>
              ) : steps.includes("send") ? (
                <button className="primary" autoFocus onClick={start}>
                  Continue
                </button>
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
                      <CoverImage book={book} />
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
            <ul className="review-covers">
              {wanted.map((id) => {
                const book = books.get(id);
                const row = covers.get(id);
                const set = (patch: Partial<CoverRow>) =>
                  row &&
                  setCovers((c) => new Map(c).set(id, { ...row, ...patch }));
                const offered = row?.picked ?? row?.best?.candidate.url ?? null;
                return (
                  <li key={id}>
                    <div className="row-title">{title(id)}</div>
                    <div className="row-sub">{book?.authors.join(", ")}</div>
                    <div className="review-cover-options">
                      <CoverOption
                        label="Current cover"
                        detail={
                          row?.current
                            ? `${row.currentWidth}×${row.current}`
                            : "None yet"
                        }
                        selected={row?.choice === "keep"}
                        onSelect={() => set({ choice: "keep" })}
                      >
                        <CoverImage book={book} />
                      </CoverOption>
                      {!row ? (
                        <div className="review-cover-searching muted">
                          Searching for covers…
                        </div>
                      ) : (
                        <>
                          {offered && (
                            <CoverOption
                              label="New cover"
                              detail={
                                row.picked
                                  ? "Your choice"
                                  : `${row.best?.candidate.source} · ${row.best?.width}×${row.best?.height}`
                              }
                              selected={row.choice === "use"}
                              onSelect={() => set({ choice: "use" })}
                            >
                              <CoverImage book={book} src={offered} />
                            </CoverOption>
                          )}
                          {canUpscale && row.current !== null && (
                            <CoverOption
                              label="Upscaled cover"
                              detail="You'll compare it first"
                              selected={row.choice === "upscale"}
                              onSelect={() => set({ choice: "upscale" })}
                            >
                              <CoverImage book={book} />
                            </CoverOption>
                          )}
                          <button
                            className="review-cover-more"
                            onClick={() => setPicking(id)}
                          >
                            See all covers…
                          </button>
                        </>
                      )}
                    </div>
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

        {screen === "send" && (
          <>
            <ul className="enhance-list review-send">
              {sendList
                .filter((id) => !removed.has(id))
                .map((id) => {
                  const book = books.get(id);
                  const status = sending ? statuses[id] : undefined;
                  const doubtful = review.findings.find(
                    (f) =>
                      f.book_id === id &&
                      (f.issue === "damaged" || f.issue === "drm"),
                  );
                  return (
                    <li key={id}>
                      <label>
                        <input
                          type="checkbox"
                          disabled={sending}
                          checked={toSend.has(id)}
                          onChange={() =>
                            setToSend((t) => {
                              const next = new Set(t);
                              if (!next.delete(id)) next.add(id);
                              return next;
                            })
                          }
                        />
                        <span className="thumb">
                          <CoverImage book={book} />
                        </span>
                        <span className="enhance-book">
                          <span className="row-title">{title(id)}</span>
                          <span className="row-sub">
                            {book?.authors.join(", ")}
                          </span>
                          {doubtful && (
                            <span className="unsure">
                              {doubtful.issue === "drm" ? "has DRM" : "damaged"}
                            </span>
                          )}
                        </span>
                        {status && (
                          <span
                            className={
                              status.stage === "failed"
                                ? "notice-error"
                                : "muted"
                            }
                            title={
                              status.stage === "failed"
                                ? status.error
                                : undefined
                            }
                          >
                            {sendStatusLabel(status)}
                          </span>
                        )}
                      </label>
                    </li>
                  );
                })}
            </ul>
            <div className="form-actions">
              <button
                className="secondary"
                disabled={sending}
                onClick={() => advance("send")}
              >
                {kobo ? "Not now" : "Continue"}
              </button>
              {kobo && (
                <button
                  className="primary"
                  autoFocus
                  disabled={sending || toSend.size === 0}
                  onClick={sendBooks}
                >
                  {sending ? "Sending…" : `Send ${plural(toSend.size, "book")}`}
                </button>
              )}
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
              {done.sent > 0 && (
                <li>Sent {plural(done.sent, "book")} to your Kobo</li>
              )}
              {unsent.length > 0 && (
                <li className="warning">
                  <div>
                    <div className="review-line">
                      {plural(unsent.length, "book")} couldn't be sent
                    </div>
                    <div className="muted">{unsent.join(", ")}</div>
                  </div>
                </li>
              )}
              {done.details +
                done.covers +
                done.upscaled +
                done.sent +
                removed.size +
                unsent.length ===
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

/** One cover to choose, big enough to judge, saying what it is. */
function CoverOption({
  label,
  detail,
  selected,
  onSelect,
  children,
}: {
  label: string;
  detail: string;
  selected: boolean;
  onSelect: () => void;
  children: ReactNode;
}) {
  return (
    <button
      className={`review-cover-option ${selected ? "selected" : ""}`}
      aria-pressed={selected}
      onClick={onSelect}
    >
      <span className="review-cover-label">{label}</span>
      <span className="review-cover-image">{children}</span>
      <span className="review-cover-detail">{detail}</span>
      <span className="review-cover-check">
        {selected ? "✓ Selected" : "\u00a0"}
      </span>
    </button>
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
