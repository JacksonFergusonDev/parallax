import { useEffect, useId, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  backupSettings,
  deleteAllBackups,
  setBackupSettings,
  showFolder,
  type BackupPolicy,
  type BackupSettings,
} from "../api";
import { Choices, Row } from "./panes";

const MB = 1_000_000;
/** Decimal, as Finder shows sizes. */
export const LIMITS = [100, 250, 500, 1000, 2000, 5000].map((n) => n * MB);

export function formatBytes(bytes: number): string {
  if (bytes >= 1000 * MB) return `${+(bytes / (1000 * MB)).toFixed(1)} GB`;
  if (bytes >= MB) return `${+(bytes / MB).toFixed(1)} MB`;
  if (bytes >= 1000) return `${Math.round(bytes / 1000)} KB`;
  return bytes === 0 ? "Nothing" : `${bytes} bytes`;
}

/** Why `next` would delete backups now, if it would. */
export function deletesNow(
  current: BackupSettings,
  next: BackupPolicy,
): string | null {
  if (next.keep === "latest" && current.policy.keep !== "latest")
    return "Older backups of each Kobo are deleted now, and changes before the latest can no longer be reverted.";
  if (next.max_bytes !== null && current.used_bytes > next.max_bytes)
    return `Backups take ${formatBytes(current.used_bytes)}, so the oldest are deleted now until they fit in ${formatBytes(next.max_bytes)}.`;
  return null;
}

export function BackupsPane({
  onError,
}: {
  onError: (message: string) => void;
}) {
  const [current, setCurrent] = useState<BackupSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const limitId = useId();

  useEffect(() => {
    let alive = true;
    backupSettings()
      .then((s) => alive && setCurrent(s))
      .catch((e) => onError(String(e)));
    return () => {
      alive = false;
    };
  }, [onError]);

  if (!current) return null;
  const { policy } = current;

  const change = async (next: BackupPolicy) => {
    const warning = deletesNow(current, next);
    if (
      warning &&
      !(await ask(warning, {
        title: "Delete older backups?",
        kind: "warning",
        okLabel: "Delete",
        cancelLabel: "Cancel",
      }))
    )
      return;
    setBusy(true);
    try {
      setCurrent(await setBackupSettings(next));
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const deleteAll = async () => {
    const sure = await ask(
      "Every backup of every Kobo is deleted, with the copies of books removed from it. Nothing done so far can be reverted afterwards.",
      {
        title: "Delete all backups?",
        kind: "warning",
        okLabel: "Delete All",
        cancelLabel: "Cancel",
      },
    );
    if (!sure) return;
    setBusy(true);
    try {
      setCurrent(await deleteAllBackups());
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const limits = LIMITS.includes(policy.max_bytes ?? -1)
    ? LIMITS
    : [...LIMITS, policy.max_bytes ?? 0].filter(Boolean).sort((a, b) => a - b);

  return (
    <>
      <p className="settings-intro">
        Before each change to a Kobo, Parallax saves a copy of its database
        here, and copies of any books it removes. Revert changes uses them to
        undo what was done.
      </p>
      <Row label="Keep">
        <Choices
          name="keep"
          value={policy.keep}
          options={[
            {
              value: "history",
              label: "Recent history",
              hint: "The last 10 changes, the first of each of the last 7 days, and removed books for 30 days.",
            },
            {
              value: "latest",
              label: "Only the latest",
              hint: "Takes the least space. Only the most recent change can be reverted.",
            },
          ]}
          onChange={(keep) => void change({ ...policy, keep })}
        />
      </Row>
      <Row
        label="Maximum size"
        labelFor={limitId}
        hint="The oldest backups go first. Each Kobo's latest backup is always kept."
      >
        <select
          id={limitId}
          value={policy.max_bytes ?? "none"}
          disabled={busy}
          onChange={(e) =>
            void change({
              ...policy,
              max_bytes:
                e.target.value === "none" ? null : Number(e.target.value),
            })
          }
        >
          {limits.map((bytes) => (
            <option key={bytes} value={bytes}>
              {formatBytes(bytes)}
            </option>
          ))}
          <option value="none">No limit</option>
        </select>
      </Row>
      <Row label="In use">
        <p className="settings-value">
          {formatBytes(current.used_bytes)}
          {policy.max_bytes !== null && ` of ${formatBytes(policy.max_bytes)}`}
        </p>
        <div className="settings-inline">
          <button onClick={() => void showFolder("backups")}>
            Show in Finder
          </button>
          <button
            className="danger"
            disabled={busy || current.used_bytes === 0}
            onClick={() => void deleteAll()}
          >
            Delete All Backups…
          </button>
        </div>
      </Row>
    </>
  );
}
