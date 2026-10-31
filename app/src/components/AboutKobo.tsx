import { useEffect, useState } from "react";
import {
  deviceHistory,
  type DeviceHistory,
  type DeviceInfo,
  type LogEntry,
} from "../api";

interface Props {
  device: DeviceInfo;
  /** Titles of the books on the device, by ContentID. */
  titles: Map<string, string>;
  onClose: () => void;
}

const OP_LABELS: Record<string, string> = {
  send: "Sent",
  replace: "Updated",
  remove: "Removed",
  collection: "Collections",
  series: "Series",
  tidy: "Tidied",
  restore: "Restored",
};

const when = (iso: string) =>
  new Date(iso).toLocaleString(undefined, {
    day: "numeric",
    month: "short",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });

/** "KEpub/Dune - Frank Herbert.kepub.epub" → "Dune - Frank Herbert". */
const bookName = (contentId: string) =>
  (contentId.split("/").pop() ?? contentId).replace(/(\.kepub)?\.epub$/i, "");

function describe(e: LogEntry, titles: Map<string, string>): string {
  const ids = e.content_ids ?? [];
  const books =
    ids.length === 1
      ? (titles.get(ids[0]) ?? bookName(ids[0]))
      : ids.length > 1
        ? `${ids.length} books`
        : "";
  if (e.detail && books && e.op !== "replace") return `${e.detail}: ${books}`;
  return e.detail ?? books;
}

/** ERC's records on the device: who wrote what, with which version, and when. */
export function AboutKobo({ device, titles, onClose }: Props) {
  const [history, setHistory] = useState<DeviceHistory | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    deviceHistory(device.mount)
      .then((h) => alive && setHistory(h))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [device.mount]);

  const d = history?.device;
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal about" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <div>
            <h2>About this Kobo</h2>
            <p className="muted">
              E-Reader Companion keeps a record of its changes in a hidden .erc
              folder on the Kobo.
            </p>
          </div>
        </div>
        <dl className="facts about-facts">
          <dt>Model</dt>
          <dd>{device.model}</dd>
          <dt>Firmware</dt>
          <dd>{device.firmware}</dd>
          <dt>Serial</dt>
          <dd>{device.serial}</dd>
          {d && (
            <>
              <dt>First change</dt>
              <dd>
                {when(d.first_write.at)} · version {d.first_write.erc_version}
              </dd>
              <dt>Latest change</dt>
              <dd>
                {when(d.last_write.at)} · version {d.last_write.erc_version}
                {d.last_write.firmware !== device.firmware &&
                  ` · firmware was ${d.last_write.firmware}`}
              </dd>
            </>
          )}
          {history && (
            <>
              <dt>Books from ERC</dt>
              <dd>
                {history.books.length}
                {history.changed.length > 0 &&
                  ` · ${history.changed.length} changed by something else since`}
                {history.missing.length > 0 &&
                  ` · ${history.missing.length} no longer on the Kobo`}
              </dd>
            </>
          )}
        </dl>
        {history && !d && (
          <p className="muted">
            E-Reader Companion hasn't changed this Kobo yet.
          </p>
        )}
        {error && <p className="notice notice-error">{error}</p>}
        {history?.problems.map((p) => (
          <p key={p} className="notice notice-error">
            Couldn't read part of the record ({p}). It is rebuilt on the next
            change.
          </p>
        ))}
        {history && history.log.length > 0 && (
          <>
            <h3 className="about-heading">Activity</h3>
            <ul className="activity">
              {history.log.map((e, i) => (
                <li
                  key={`${e.at}-${i}`}
                  className={e.error ? "failed" : ""}
                  title={
                    e.backup
                      ? `Database backed up first: ${e.backup}`
                      : undefined
                  }
                >
                  <span className="activity-when muted">{when(e.at)}</span>
                  <span className="activity-op">{OP_LABELS[e.op] ?? e.op}</span>
                  <span className="activity-what">
                    {describe(e, titles)}
                    {e.error && (
                      <span className="activity-error">Failed: {e.error}</span>
                    )}
                  </span>
                </li>
              ))}
            </ul>
          </>
        )}
        <div className="form-actions">
          <button className="primary" onClick={onClose} autoFocus>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
