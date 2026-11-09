import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import {
  deviceHistory,
  saveDeviceReport,
  type DeviceHistory,
  type DeviceInfo,
  type LogEntry,
  type Screen,
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

/** "1264 × 1680, black and white", saying when the size is a guess. */
export function screenText(screen: Screen): string {
  const size = `${screen.width} × ${screen.height}, ${
    screen.colour ? "colour" : "black and white"
  }`;
  if (screen.source === "covers") return `${size} (measured from its covers)`;
  if (screen.source === "assumed")
    return `${size} (assumed, as this model isn't known yet)`;
  return size;
}

/** Parallax's records on the device: who wrote what, with which version, and when. */
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

  const [report, setReport] = useState<{
    error: boolean;
    text: string;
  } | null>(null);
  const saveReport = async () => {
    const model = device.model.replace(/[^\w ]+/g, "").trim();
    const path = await save({
      defaultPath: `${model} device report.json`,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!path) return;
    try {
      await saveDeviceReport(device.mount, path);
      setReport({ error: false, text: `Saved to ${path}` });
    } catch (e) {
      setReport({ error: true, text: String(e) });
    }
  };

  const d = history?.device;
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal about" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <div>
            <h2>About this Kobo</h2>
            <p className="muted">
              Parallax keeps a record of its changes in a hidden .parallax
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
          <dt>Screen</dt>
          <dd className="wrap">{screenText(device.screen)}</dd>
          <dt>Changes</dt>
          <dd className="wrap">
            {device.read_only
              ? `Read-only: ${device.read_only}`
              : "Allowed, with a backup first"}
          </dd>
          {d && (
            <>
              <dt>First change</dt>
              <dd>
                {when(d.first_write.at)} · version{" "}
                {d.first_write.parallax_version}
              </dd>
              <dt>Latest change</dt>
              <dd>
                {when(d.last_write.at)} · version{" "}
                {d.last_write.parallax_version}
                {d.last_write.firmware !== device.firmware &&
                  ` · firmware was ${d.last_write.firmware}`}
              </dd>
            </>
          )}
          {history && (
            <>
              <dt>Books from Parallax</dt>
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
          <p className="muted">Parallax hasn't changed this Kobo yet.</p>
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
        <h3 className="about-heading">Device report</h3>
        <p className="muted">
          A file describing this Kobo's model, firmware and database layout, for
          adding support for Kobos Parallax doesn't know yet. It has no book
          titles, serial number, reading data or highlights, and saving it only
          reads the Kobo.
        </p>
        {report && (
          <p className={`notice notice-${report.error ? "error" : "info"}`}>
            {report.text}
          </p>
        )}
        <div className="form-actions">
          <button className="secondary" onClick={saveReport}>
            Save device report…
          </button>
          <button className="primary" onClick={onClose} autoFocus>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
