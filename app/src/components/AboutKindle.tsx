import { useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { saveDeviceReport, type DeviceInfo } from "../api";
import { screenText } from "./AboutKobo";

interface Props {
  device: DeviceInfo;
  /** How many books are on it, once read. */
  books: number | null;
  onClose: () => void;
}

/** What Parallax knows about a connected Kindle, and its device report. */
export function AboutKindle({ device, books, onClose }: Props) {
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
  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal about" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <div>
            <h2>About this Kindle</h2>
            <p className="muted">
              Kindle support is new and hasn't been tested on a Kindle yet.
              Parallax adds and removes books in the Kindle's documents folder,
              and reads your progress and highlights.
            </p>
          </div>
        </div>
        <dl className="facts about-facts">
          <dt>Model</dt>
          <dd>{device.model}</dd>
          <dt>Firmware</dt>
          <dd>{device.firmware || "Not reported"}</dd>
          <dt>Serial</dt>
          <dd>{device.serial}</dd>
          <dt>Connection</dt>
          <dd className="wrap">
            {device.transport === "mtp"
              ? "MTP, which recent Kindles use instead of appearing as a drive"
              : "As a drive"}
          </dd>
          <dt>Screen</dt>
          <dd className="wrap">{screenText(device.screen)}</dd>
          <dt>Hardware checks</dt>
          <dd className="wrap">Not yet run on any Kindle</dd>
          {books !== null && (
            <>
              <dt>Books</dt>
              <dd>{books}</dd>
            </>
          )}
        </dl>
        <h3 className="about-heading">Device report</h3>
        <p className="muted">
          A file describing this Kindle's model, firmware, connection and the
          kinds of files on it, for improving Kindle support. It has no book
          titles, serial number, reading data or highlights, and saving it only
          reads the Kindle.
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
