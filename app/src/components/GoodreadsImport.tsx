import { useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  goodreadsImport,
  goodreadsPlan,
  type GoodreadsPlan,
  type GoodreadsReport,
  type GoodreadsResolutions,
} from "../api";
import { goodreadsSummary, plural } from "../goodreads";

interface Props {
  onClose: () => void;
}

type Pick = "mine" | "goodreads";
type MissingPick = "keep" | "remove";

/** A two-way choice that reads as one control. */
function Choice<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: [T, string][];
  onChange: (value: T) => void;
}) {
  return (
    <div className="goodreads-choice" role="radiogroup" aria-label={label}>
      {options.map(([option, text]) => (
        <button
          key={option}
          type="button"
          role="radio"
          className="secondary"
          aria-checked={value === option}
          onClick={() => onChange(option)}
        >
          {text}
        </button>
      ))}
    </div>
  );
}

/**
 * Imports a Goodreads export. Importing a newer one updates what's saved. If
 * a book was changed both here and on Goodreads, or Goodreads no longer has
 * it, you decide, or take the recommended choice for everything in one click.
 */
export function GoodreadsImport({ onClose }: Props) {
  const [path, setPath] = useState<string | null>(null);
  const [plan, setPlan] = useState<GoodreadsPlan | null>(null);
  const [picks, setPicks] = useState<Record<string, Pick>>({});
  const [missingPicks, setMissingPicks] = useState<Record<string, MissingPick>>(
    {},
  );
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<GoodreadsReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  const choose = async () => {
    const picked = await open({
      title: "Choose your Goodreads export",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (typeof picked !== "string") return;
    setBusy(true);
    setError(null);
    setPlan(null);
    try {
      setPlan(await goodreadsPlan(picked));
      setPicks({});
      setMissingPicks({});
      setPath(picked);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const run = async (resolutions: GoodreadsResolutions) => {
    if (!path) return;
    setBusy(true);
    setError(null);
    try {
      setReport(await goodreadsImport(path, resolutions));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const needsDecisions =
    (plan?.conflicts.length ?? 0) + (plan?.missing.length ?? 0) > 0;
  const recommended: GoodreadsResolutions = { conflicts: {}, missing: {} };
  const chosen: GoodreadsResolutions = {
    conflicts: Object.fromEntries(
      (plan?.conflicts ?? []).map((c) => [
        c.goodreads_id,
        picks[c.goodreads_id] ?? "mine",
      ]),
    ),
    missing: Object.fromEntries(
      (plan?.missing ?? []).map((m) => [
        m.goodreads_id,
        missingPicks[m.goodreads_id] ?? "keep",
      ]),
    ),
  };
  const removing = Object.values(chosen.missing).filter(
    (v) => v === "remove",
  ).length;
  const setAllMissing = (value: MissingPick) =>
    setMissingPicks(
      Object.fromEntries(
        (plan?.missing ?? []).map((m) => [m.goodreads_id, value]),
      ),
    );

  return (
    <div className="modal-backdrop" onClick={busy ? undefined : onClose}>
      <div
        className="modal goodreads-import"
        role="dialog"
        aria-label="Import from Goodreads"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <div>
            <h2>Import from Goodreads</h2>
            <p className="muted">
              Choose the CSV from Goodreads (My Books, then Import and export).
              Import a newer one any time and your books update without being
              added twice.
            </p>
          </div>
        </div>

        {error && <p className="notice notice-error wrap">{error}</p>}

        {report ? (
          <p>{goodreadsSummary(report)}</p>
        ) : plan ? (
          <>
            <p>{goodreadsSummary(plan)}</p>
            {needsDecisions && (
              <>
                <div className="goodreads-recommend">
                  <span>
                    {plural(
                      plan.conflicts.length + plan.missing.length,
                      "book",
                    )}{" "}
                    need your decision. Not sure? Keep your changes and keep
                    books Goodreads no longer lists.
                  </span>
                  <button
                    className="primary"
                    disabled={busy}
                    onClick={() => run(recommended)}
                  >
                    Use recommended
                  </button>
                </div>
                <div className="goodreads-decisions">
                  {plan.conflicts.length > 0 && (
                    <section>
                      <h3>You and Goodreads both changed these</h3>
                      {plan.conflicts.map((c) => (
                        <div key={c.goodreads_id} className="goodreads-item">
                          <span className="goodreads-title" title={c.title}>
                            {c.title}
                            <span className="muted">
                              {c.authors[0] && ` · ${c.authors[0]}`}
                            </span>
                          </span>
                          <Choice
                            label={`Which to keep for ${c.title}`}
                            value={picks[c.goodreads_id] ?? "mine"}
                            options={[
                              ["mine", "Keep mine"],
                              ["goodreads", "Use Goodreads"],
                            ]}
                            onChange={(v) =>
                              setPicks({ ...picks, [c.goodreads_id]: v })
                            }
                          />
                          <ul className="goodreads-changes">
                            {c.changes.map((f) => (
                              <li key={f.field}>
                                {f.label}: you have {f.mine}, Goodreads has{" "}
                                {f.theirs}
                              </li>
                            ))}
                          </ul>
                        </div>
                      ))}
                    </section>
                  )}
                  {plan.missing.length > 0 && (
                    <section>
                      <h3>
                        {plural(plan.missing.length, "book")} not in this export
                      </h3>
                      <p className="muted">
                        You saved these from an earlier export. Keep them as
                        books of your own, or remove them.
                      </p>
                      <div className="goodreads-bulk">
                        <button
                          className="link"
                          onClick={() => setAllMissing("keep")}
                        >
                          Keep all
                        </button>
                        <button
                          className="link"
                          onClick={() => setAllMissing("remove")}
                        >
                          Remove all
                        </button>
                      </div>
                      {plan.missing.map((m) => (
                        <div key={m.goodreads_id} className="goodreads-item">
                          <span className="goodreads-title" title={m.title}>
                            {m.title}
                            <span className="muted">{` · ${m.status}`}</span>
                          </span>
                          <Choice
                            label={`What to do with ${m.title}`}
                            value={missingPicks[m.goodreads_id] ?? "keep"}
                            options={[
                              ["keep", "Keep"],
                              ["remove", "Remove"],
                            ]}
                            onChange={(v) =>
                              setMissingPicks({
                                ...missingPicks,
                                [m.goodreads_id]: v,
                              })
                            }
                          />
                        </div>
                      ))}
                    </section>
                  )}
                </div>
              </>
            )}
          </>
        ) : (
          busy && <p className="muted">Reading the export…</p>
        )}

        <div className="form-actions">
          <span className="spacer" />
          {report ? (
            <button className="primary" onClick={onClose} autoFocus>
              Done
            </button>
          ) : (
            <>
              <button className="secondary" onClick={onClose} disabled={busy}>
                Cancel
              </button>
              {plan ? (
                <>
                  <button
                    className="secondary"
                    onClick={choose}
                    disabled={busy}
                  >
                    Choose another…
                  </button>
                  <button
                    className="primary"
                    disabled={busy}
                    onClick={() => run(needsDecisions ? chosen : recommended)}
                  >
                    {removing > 0
                      ? `Import and remove ${plural(removing, "book")}`
                      : "Import"}
                  </button>
                </>
              ) : (
                <button className="primary" onClick={choose} disabled={busy}>
                  Choose export…
                </button>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
