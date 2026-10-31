import { useCallback, useEffect, useState } from "react";
import {
  rulesApply,
  rulesDelete,
  rulesList,
  rulesPreview,
  rulesSave,
  type PlannedCollection,
  type Rule,
  type RuleKind,
} from "../api";

const KIND_LABEL: Record<RuleKind, string> = {
  series: "A collection for each series",
  tag: "A collection for each tag",
  author: "A collection for each author",
};

const DEFAULT_MIN: Record<RuleKind, number> = { series: 2, tag: 1, author: 3 };

interface Props {
  mount: string;
  onApplied: (added: number) => void;
  onClose: () => void;
}

/** Automatic collections: rules plus a live preview of what they'd add. */
export function RulesDialog({ mount, onApplied, onClose }: Props) {
  const [rules, setRules] = useState<Rule[] | null>(null);
  const [plan, setPlan] = useState<PlannedCollection[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(
    () =>
      Promise.all([rulesList(), rulesPreview(mount)])
        .then(([r, p]) => {
          setRules(r);
          setPlan(p);
        })
        .catch((e) => setError(String(e))),
    [mount],
  );

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const save = async (rule: Rule) => {
    setError(null);
    try {
      await rulesSave(rule);
      await refresh();
    } catch (e) {
      setError(String(e));
    }
  };

  const remove = async (id: number) => {
    await rulesDelete(id);
    await refresh();
  };

  const apply = async () => {
    setBusy(true);
    setError(null);
    try {
      const added = await rulesApply(mount);
      onApplied(added);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const unused = (Object.keys(KIND_LABEL) as RuleKind[]).filter(
    (k) => !rules?.some((r) => r.kind === k),
  );
  const additions = (plan ?? []).reduce((n, p) => n + p.content_ids.length, 0);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div
        className="modal rules"
        role="dialog"
        aria-label="Automatic collections"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal-head">
          <div>
            <h2>Automatic collections</h2>
            <p className="muted">
              Enabled rules run when you send books and when your Kobo connects.
              They only add books to collections; they never remove books or
              delete collections.
            </p>
          </div>
          <button className="icon" aria-label="Close" onClick={onClose}>
            ×
          </button>
        </div>

        <ul className="rule-list">
          {rules?.map((r) => (
            <li key={r.id} className={r.enabled ? "" : "disabled"}>
              <label className="rule-toggle">
                <input
                  type="checkbox"
                  checked={r.enabled}
                  onChange={(e) => save({ ...r, enabled: e.target.checked })}
                />
                {KIND_LABEL[r.kind]}
              </label>
              <label className="rule-field">
                with at least
                <input
                  type="number"
                  min={1}
                  value={r.min_books}
                  onChange={(e) =>
                    save({
                      ...r,
                      min_books: Math.max(1, Number(e.target.value) || 1),
                    })
                  }
                />
                {r.min_books === 1 ? "book" : "books"}
              </label>
              <label className="rule-field">
                named
                <input
                  className="prefix"
                  placeholder="prefix"
                  defaultValue={r.prefix}
                  onBlur={(e) =>
                    e.target.value !== r.prefix &&
                    save({ ...r, prefix: e.target.value })
                  }
                />
                <span className="muted">+ {r.kind}</span>
              </label>
              <button
                className="icon"
                aria-label="Delete rule"
                onClick={() => remove(r.id)}
              >
                ×
              </button>
            </li>
          ))}
        </ul>
        {unused.length > 0 && (
          <div className="rule-add">
            {unused.map((k) => (
              <button
                key={k}
                className="secondary"
                onClick={() =>
                  save({
                    id: 0,
                    kind: k,
                    enabled: true,
                    min_books: DEFAULT_MIN[k],
                    prefix: "",
                  })
                }
              >
                + {KIND_LABEL[k]}
              </button>
            ))}
          </div>
        )}

        <h3>On this Kobo</h3>
        {plan === null && !error && <p className="muted">Working it out…</p>}
        {plan?.length === 0 && (
          <p className="muted">
            Nothing to do: everything is already in its collections.
          </p>
        )}
        {plan && plan.length > 0 && (
          <ul className="plan-list">
            {plan.map((p) => (
              <li key={p.name} title={p.titles.join("\n")}>
                <span className="plan-name">
                  {p.new ? "Create" : "Add to"} <strong>{p.name}</strong>
                </span>
                <span className="muted">
                  {p.content_ids.length}{" "}
                  {p.content_ids.length === 1 ? "book" : "books"}
                </span>
              </li>
            ))}
          </ul>
        )}
        {error && <p className="notice notice-error">{error}</p>}
        <div className="form-actions">
          <button className="secondary" onClick={onClose}>
            Done
          </button>
          <button
            className="primary"
            disabled={busy || additions === 0}
            onClick={apply}
          >
            {busy
              ? "Applying…"
              : additions
                ? `Apply now (${additions} ${additions === 1 ? "book" : "books"})`
                : "Apply now"}
          </button>
        </div>
      </div>
    </div>
  );
}
