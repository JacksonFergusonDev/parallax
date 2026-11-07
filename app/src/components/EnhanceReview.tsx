import { useEffect, useMemo, useState } from "react";
import {
  coverSrc,
  enhanceApply,
  enhancePropose,
  getSetting,
  listBooks,
  onEnhanceProgress,
  setSetting,
  type Book,
  type ImportProgress,
  type Proposal,
} from "../api";
import {
  countChanges,
  initialSelection,
  selectAll,
  toAccepted,
  toggleGenre,
  toggleSeries,
  withChanges,
  type Choice,
  type Selection,
} from "../enhance";

interface Props {
  ids: number[];
  /** A connected Kobo whose series and folders are used as clues. */
  mount: string | null;
  onApplied: (updated: Book[]) => void;
  /** Offered once changes are saved, for books with a copy on the Kobo. */
  kobo?: {
    isOnKobo: (id: number) => boolean;
    update: (ids: number[]) => Promise<void>;
    /** Opens automatic collections. */
    collections?: () => void;
  };
  onClose: () => void;
}

type Phase =
  | { kind: "looking"; progress: ImportProgress | null }
  | {
      kind: "review";
      proposals: Proposal[];
      problems: string[];
      books: Map<number, Book>;
    }
  | { kind: "saving" }
  | { kind: "done"; updated: Book[] }
  | { kind: "error"; message: string };

/** Looks up series and genres, lets the user pick, and saves the picks. */
export function EnhanceReview({ ids, mount, onApplied, kobo, onClose }: Props) {
  const [phase, setPhase] = useState<Phase>({
    kind: "looking",
    progress: null,
  });
  const [selection, setSelection] = useState<Selection>(new Map());

  useEffect(() => {
    let alive = true;
    const unlisten = onEnhanceProgress(
      (p) => alive && setPhase({ kind: "looking", progress: p }),
    );
    Promise.all([enhancePropose(ids, mount), listBooks()])
      .then(([all, library]) => {
        if (!alive) return;
        const proposals = withChanges(all);
        const problems = [...new Set(all.flatMap((p) => p.problems))];
        const books = new Map(library.map((b) => [b.id, b]));
        setSelection(initialSelection(proposals));
        setPhase({ kind: "review", proposals, problems, books });
      })
      .catch((e) => alive && setPhase({ kind: "error", message: String(e) }))
      .finally(() => unlisten.then((f) => f()));
    return () => {
      alive = false;
      unlisten.then((f) => f());
    };
  }, [ids, mount]);

  const save = async (proposals: Proposal[]) => {
    setPhase({ kind: "saving" });
    try {
      const updated = await enhanceApply(toAccepted(proposals, selection));
      onApplied(updated);
      setPhase({ kind: "done", updated });
    } catch (e) {
      setPhase({ kind: "error", message: String(e) });
    }
  };

  const plural = (n: number, word: string) =>
    `${n} ${word}${n === 1 ? "" : "s"}`;

  return (
    <div
      className="modal-backdrop"
      onClick={phase.kind === "saving" ? undefined : onClose}
    >
      <div className="modal enhance" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <div>
            <h2>Find series and genres</h2>
            <p className="muted">
              {phase.kind === "looking"
                ? phase.progress
                  ? `Looking up ${phase.progress.index} of ${phase.progress.total}: ${phase.progress.title}`
                  : `Looking up ${plural(ids.length, "book")}…`
                : phase.kind === "review"
                  ? phase.proposals.length
                    ? `${plural(phase.proposals.length, "book")} can be filled in. Unsure matches aren't ticked.`
                    : "Nothing to add: these books already have series and genres, or no source knew them."
                  : phase.kind === "done"
                    ? `Updated ${plural(phase.updated.length, "book")}.`
                    : phase.kind === "saving"
                      ? "Saving…"
                      : ""}
            </p>
          </div>
        </div>

        {phase.kind === "looking" && phase.progress && (
          <progress
            className="enhance-progress"
            value={phase.progress.index}
            max={phase.progress.total}
          />
        )}
        {phase.kind === "error" && (
          <p className="notice notice-error">{phase.message}</p>
        )}

        {phase.kind === "review" && (
          <>
            {phase.proposals.length > 0 && (
              <div className="enhance-bulk">
                <button
                  className="link"
                  onClick={() => setSelection(selectAll(phase.proposals, true))}
                >
                  Tick all
                </button>
                <button
                  className="link"
                  onClick={() =>
                    setSelection(selectAll(phase.proposals, false))
                  }
                >
                  Untick all
                </button>
              </div>
            )}
            <ul className="enhance-list">
              {phase.proposals.map((p) => {
                const book = phase.books.get(p.book_id);
                const choice = selection.get(p.book_id);
                const src = book ? coverSrc(book) : null;
                return (
                  <li key={p.book_id}>
                    <span className="thumb">
                      {src ? <img src={src} alt="" loading="lazy" /> : null}
                    </span>
                    <div className="enhance-book">
                      <div className="row-title">
                        {book?.title ?? `Book ${p.book_id}`}
                      </div>
                      <div className="row-sub">{book?.authors.join(", ")}</div>
                      <ProposalChanges
                        proposal={p}
                        choice={choice}
                        onToggleSeries={() =>
                          setSelection((s) => toggleSeries(s, p.book_id))
                        }
                        onToggleGenre={(g) =>
                          setSelection((s) => toggleGenre(s, p.book_id, g))
                        }
                      />
                    </div>
                  </li>
                );
              })}
            </ul>
            {phase.problems.length > 0 && (
              <p className="muted enhance-problems">
                Some sources didn't answer: {phase.problems.join("; ")}
              </p>
            )}
            <GoogleKey />
            <div className="form-actions">
              <button className="secondary" onClick={onClose}>
                Cancel
              </button>
              <button
                className="primary"
                disabled={countChanges(selection) === 0}
                onClick={() => save(phase.proposals)}
              >
                {`Save ${plural(countChanges(selection), "change")}`}
              </button>
            </div>
          </>
        )}

        {phase.kind === "done" && (
          <DoneActions updated={phase.updated} kobo={kobo} onClose={onClose} />
        )}
        {(phase.kind === "error" || phase.kind === "looking") && (
          <div className="form-actions">
            <button className="secondary" onClick={onClose}>
              {phase.kind === "looking" ? "Cancel" : "Close"}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

/** The ticks for one book's proposed series and genres. */
export function ProposalChanges({
  proposal,
  choice,
  onToggleSeries,
  onToggleGenre,
}: {
  proposal: Proposal;
  choice: Choice | undefined;
  onToggleSeries: () => void;
  onToggleGenre: (genre: string) => void;
}) {
  const { series, genres } = proposal;
  return (
    <>
      {series && (
        <label className="enhance-change">
          <input
            type="checkbox"
            checked={choice?.series ?? false}
            onChange={onToggleSeries}
          />
          <span className="enhance-field">Series</span>
          <span>
            {series.name}
            {series.index ? ` #${series.index}` : ""}
          </span>
          <span className="muted">{series.source}</span>
          {!series.confident && <span className="unsure">unsure</span>}
        </label>
      )}
      {genres.map((g) => (
        <label key={g.name} className="enhance-change">
          <input
            type="checkbox"
            checked={choice?.genres.has(g.name) ?? false}
            onChange={() => onToggleGenre(g.name)}
          />
          <span className="enhance-field">Genre</span>
          <span>{g.name}</span>
          <span className="muted">{g.sources.join(", ")}</span>
          {!g.confident && <span className="unsure">unsure</span>}
        </label>
      ))}
    </>
  );
}

function DoneActions({
  updated,
  kobo,
  onClose,
}: {
  updated: Book[];
  kobo: Props["kobo"];
  onClose: () => void;
}) {
  const onKobo = useMemo(
    () =>
      kobo ? updated.filter((b) => kobo.isOnKobo(b.id)).map((b) => b.id) : [],
    [updated, kobo],
  );
  const [state, setState] = useState<"idle" | "updating" | "updated" | string>(
    "idle",
  );
  return (
    <>
      {kobo && onKobo.length > 0 && (
        <p className="muted">
          {state === "updated"
            ? "Your Kobo has the changes. Eject it to see them."
            : `${onKobo.length} of these ${onKobo.length === 1 ? "is" : "are"} on your Kobo. Update ${onKobo.length === 1 ? "it" : "them"} so the Kobo shows the new series; reading progress is kept.`}
        </p>
      )}
      {state !== "idle" && state !== "updating" && state !== "updated" && (
        <p className="notice notice-error">{state}</p>
      )}
      <div className="form-actions">
        {kobo?.collections && (
          <button className="link" onClick={kobo.collections}>
            Make collections from series and genres…
          </button>
        )}
        <span className="spacer" />
        {kobo && onKobo.length > 0 && state !== "updated" && (
          <button
            className="secondary"
            disabled={state === "updating"}
            onClick={() => {
              setState("updating");
              kobo
                .update(onKobo)
                .then(() => setState("updated"))
                .catch((e) => setState(String(e)));
            }}
          >
            {state === "updating"
              ? "Updating…"
              : `Update ${onKobo.length} on Kobo`}
          </button>
        )}
        <button className="primary" onClick={onClose} autoFocus>
          Done
        </button>
      </div>
    </>
  );
}

/** Optional Google Books API key; more categories for more books. */
function GoogleKey() {
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState("");
  const [saved, setSaved] = useState<boolean | null>(null);

  useEffect(() => {
    getSetting("google_books_key")
      .then((k) => setSaved(!!k))
      .catch(() => setSaved(false));
  }, []);

  if (saved === null) return null;
  if (!open)
    return (
      <p className="muted enhance-key">
        {saved
          ? "Google Books is used with your API key. "
          : "Add a free Google Books API key to look up categories for more books. "}
        <button className="link" onClick={() => setOpen(true)}>
          {saved ? "Change key" : "Add key"}
        </button>
      </p>
    );
  return (
    <form
      className="enhance-key"
      onSubmit={(e) => {
        e.preventDefault();
        setSetting("google_books_key", value || null).then(() => {
          setSaved(!!value);
          setOpen(false);
        });
      }}
    >
      <input
        type="password"
        placeholder="Google Books API key (used from the next lookup)"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        autoFocus
      />
      <button className="secondary" type="submit">
        {value ? "Save" : "Remove key"}
      </button>
    </form>
  );
}
