import { useEffect, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import type { DuplicateReason } from "../api";

/** One copy in a group, already described for display. */
export interface ReviewMember {
  id: string;
  title: string;
  subtitle: string;
  cover: string | null;
  /** Short facts shown under the title: format, size, progress… */
  facts: string[];
  /** Why this copy can't be removed here, if it can't. */
  locked?: string;
  /** Suggested for removal. */
  remove: boolean;
}

export interface ReviewGroup {
  key: string;
  reasons: DuplicateReason[];
  members: ReviewMember[];
}

interface Props {
  /** "library" or "Kobo", for wording. */
  place: string;
  /** Started when the review opens, so later list refreshes don't reshuffle it. */
  groups: Promise<ReviewGroup[]>;
  /** Shown in the confirmation before removing. */
  removeWarning: string;
  remove: (ids: string[]) => Promise<void>;
  dismiss: (key: string) => Promise<void>;
  onChanged: () => void;
  onClose: () => void;
}

const REASONS: Record<DuplicateReason, string> = {
  same_file: "Identical file",
  same_isbn: "Same ISBN",
  same_title_and_author: "Same title and author",
};

/** Walks through probable duplicates one group at a time. */
export function DuplicateReview({
  place,
  groups: pending,
  removeWarning,
  remove,
  dismiss,
  onChanged,
  onClose,
}: Props) {
  const [groups, setGroups] = useState<ReviewGroup[] | null>(null);
  const [index, setIndex] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    pending
      .then((g) => alive && setGroups(g))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [pending]);

  const group = groups?.[index];
  const close = () => {
    if (index > 0) onChanged();
    onClose();
  };

  return (
    <div className="modal-backdrop" onClick={close}>
      <div className="modal duplicates" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <div>
            <h2>Duplicates in your {place}</h2>
            <p className="muted">
              {groups === null
                ? "Looking for duplicates…"
                : groups.length === 0
                  ? "No duplicates found."
                  : group
                    ? `Group ${index + 1} of ${groups.length} · ${group.reasons.map((r) => REASONS[r]).join(" · ")}`
                    : `Reviewed all ${groups.length} ${groups.length === 1 ? "group" : "groups"}.`}
            </p>
          </div>
        </div>
        {error && <p className="notice notice-error">{error}</p>}
        {group ? (
          <GroupReview
            key={group.key}
            group={group}
            removeWarning={removeWarning}
            onRemove={async (ids) => {
              await remove(ids);
              setIndex((i) => i + 1);
            }}
            onDismiss={async () => {
              await dismiss(group.key);
              setIndex((i) => i + 1);
            }}
            onSkip={() => setIndex((i) => i + 1)}
            onError={setError}
          />
        ) : (
          <div className="form-actions">
            <button className="primary" onClick={close} autoFocus>
              Done
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

function GroupReview({
  group,
  removeWarning,
  onRemove,
  onDismiss,
  onSkip,
  onError,
}: {
  group: ReviewGroup;
  removeWarning: string;
  onRemove: (ids: string[]) => Promise<void>;
  onDismiss: () => Promise<void>;
  onSkip: () => void;
  onError: (e: string | null) => void;
}) {
  const [chosen, setChosen] = useState<Set<string>>(
    () =>
      new Set(
        group.members.filter((m) => m.remove && !m.locked).map((m) => m.id),
      ),
  );
  const [busy, setBusy] = useState(false);
  // At least one copy always stays.
  const keepsOne = chosen.size < group.members.length;

  const toggle = (id: string) =>
    setChosen((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const act = async (f: () => Promise<void>) => {
    setBusy(true);
    onError(null);
    try {
      await f();
    } catch (e) {
      onError(String(e));
      setBusy(false);
    }
  };

  const removeChosen = async () => {
    const n = chosen.size;
    const ok = await ask(
      `Remove ${n} ${n === 1 ? "copy" : "copies"}? ${removeWarning}`,
      {
        title: "Remove duplicates",
        kind: "warning",
        okLabel: "Remove",
        cancelLabel: "Cancel",
      },
    );
    if (ok) act(() => onRemove([...chosen]));
  };

  return (
    <>
      <ul className="duplicate-cards">
        {group.members.map((m) => {
          const removing = chosen.has(m.id);
          return (
            <li key={m.id} className={removing ? "removing" : ""}>
              <div className="cover">
                {m.cover ? (
                  <img src={m.cover} alt="" draggable={false} />
                ) : (
                  <div className="cover-placeholder">
                    <span>{m.title}</span>
                  </div>
                )}
              </div>
              <div className="card-title" title={m.title}>
                {m.title}
              </div>
              <div className="card-author">{m.subtitle}</div>
              <ul className="duplicate-facts">
                {m.facts.map((f) => (
                  <li key={f}>{f}</li>
                ))}
              </ul>
              {m.locked ? (
                <p className="muted duplicate-locked">{m.locked}</p>
              ) : (
                <label className="duplicate-choice">
                  <input
                    type="checkbox"
                    checked={removing}
                    onChange={() => toggle(m.id)}
                    disabled={busy}
                  />
                  Remove this copy
                </label>
              )}
            </li>
          );
        })}
      </ul>
      {!keepsOne && (
        <p className="notice notice-error">Keep at least one copy.</p>
      )}
      <div className="form-actions">
        <button
          className="link"
          onClick={() => act(onDismiss)}
          disabled={busy}
          title="Don't show these books together again"
        >
          Not duplicates
        </button>
        <span className="spacer" />
        <button className="secondary" onClick={onSkip} disabled={busy}>
          Skip
        </button>
        <button
          className="primary danger-fill"
          onClick={removeChosen}
          disabled={busy || chosen.size === 0 || !keepsOne}
        >
          {busy
            ? "Removing…"
            : chosen.size
              ? `Remove ${chosen.size}`
              : "Remove"}
        </button>
      </div>
    </>
  );
}
