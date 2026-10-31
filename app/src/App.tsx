import { useCallback, useEffect, useReducer, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  ejectDevice,
  listDevices,
  onSendProgress,
  sendToDevice,
  type DeviceInfo,
} from "./api";
import { queueReducer, sendable, type ItemStatus } from "./queue";
import "./styles.css";

const DEVICE_POLL_MS = 2000;

const STATUS_LABEL: Record<ItemStatus, string> = {
  queued: "Ready",
  converting: "Converting…",
  copying: "Copying…",
  sent: "On device",
  failed: "Failed",
};

export default function App() {
  const [device, setDevice] = useState<DeviceInfo | null>(null);
  const [items, dispatch] = useReducer(queueReducer, []);
  const [dragging, setDragging] = useState(false);
  const [busy, setBusy] = useState<"sending" | "ejecting" | null>(null);
  const [notice, setNotice] = useState<{
    kind: "error" | "info";
    text: string;
  } | null>(null);
  const batch = useRef<string[]>([]);

  // Poll for a connected Kobo. Cheap: it only stats /Volumes/*/.kobo/version.
  useEffect(() => {
    let alive = true;
    const poll = () =>
      listDevices()
        .then((devices) => alive && setDevice(devices[0] ?? null))
        .catch(() => alive && setDevice(null));
    poll();
    const timer = setInterval(poll, DEVICE_POLL_MS);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  useEffect(() => {
    const unlisten = onSendProgress((progress) =>
      dispatch({ type: "progress", batch: batch.current, progress }),
    );
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      const { type } = event.payload;
      if (type === "over" || type === "enter") setDragging(true);
      else if (type === "leave") setDragging(false);
      else if (type === "drop") {
        setDragging(false);
        dispatch({ type: "add", paths: event.payload.paths });
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const chooseFiles = useCallback(async () => {
    const picked = await open({
      multiple: true,
      filters: [{ name: "EPUB", extensions: ["epub"] }],
    });
    if (picked)
      dispatch({
        type: "add",
        paths: Array.isArray(picked) ? picked : [picked],
      });
  }, []);

  const toSend = sendable(items);
  const sentCount = items.filter((i) => i.status === "sent").length;

  const send = async () => {
    if (!device || toSend.length === 0) return;
    batch.current = toSend.map((i) => i.path);
    setBusy("sending");
    setNotice(null);
    try {
      const outcomes = await sendToDevice(device.mount, batch.current);
      const failed = outcomes.filter((o) => "Err" in o.result).length;
      setNotice(
        failed
          ? {
              kind: "error",
              text: `${failed} of ${outcomes.length} books could not be sent.`,
            }
          : {
              kind: "info",
              text: "Done. Eject the Kobo and unplug it to import the new books.",
            },
      );
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const eject = async () => {
    if (!device) return;
    setBusy("ejecting");
    try {
      await ejectDevice(device.mount);
      setDevice(null);
      dispatch({ type: "clearFinished" });
      setNotice({
        kind: "info",
        text: "Ejected. Unplug your Kobo; it will import the new books.",
      });
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className={`app ${dragging ? "dragging" : ""}`}>
      <header className="topbar">
        <h1>E-Reader Companion</h1>
        <DeviceBadge device={device} />
      </header>

      <main className="content">
        {items.length === 0 ? (
          <div className="dropzone">
            <p className="dropzone-title">Drop EPUB files here</p>
            <p className="muted">
              They're converted to KEPUB and copied to your Kobo.
            </p>
            <button className="secondary" onClick={chooseFiles}>
              Choose files…
            </button>
          </div>
        ) : (
          <ul className="queue">
            {items.map((item) => (
              <li
                key={item.path}
                className={`queue-item status-${item.status}`}
              >
                <div className="queue-text">
                  <span className="queue-name">{item.name}</span>
                  {item.error && (
                    <span className="queue-error">{item.error}</span>
                  )}
                </div>
                <span className="pill">{STATUS_LABEL[item.status]}</span>
                {(item.status === "queued" || item.status === "failed") &&
                  !busy && (
                    <button
                      className="icon"
                      aria-label={`Remove ${item.name}`}
                      onClick={() =>
                        dispatch({ type: "remove", path: item.path })
                      }
                    >
                      ×
                    </button>
                  )}
              </li>
            ))}
          </ul>
        )}
        {notice && (
          <p className={`notice notice-${notice.kind}`}>{notice.text}</p>
        )}
      </main>

      <footer className="actions">
        {items.length > 0 && (
          <button
            className="secondary"
            onClick={chooseFiles}
            disabled={busy !== null}
          >
            Add books…
          </button>
        )}
        <span className="spacer" />
        {sentCount > 0 && device && (
          <button
            className="secondary"
            onClick={eject}
            disabled={busy !== null}
          >
            {busy === "ejecting" ? "Ejecting…" : "Eject Kobo"}
          </button>
        )}
        <button
          className="primary"
          onClick={send}
          disabled={!device || toSend.length === 0 || busy !== null}
        >
          {busy === "sending"
            ? "Sending…"
            : toSend.length > 0
              ? `Send ${toSend.length} ${toSend.length === 1 ? "book" : "books"}`
              : "Send"}
        </button>
      </footer>
    </div>
  );
}

function DeviceBadge({ device }: { device: DeviceInfo | null }) {
  return device ? (
    <div className="badge connected" title={`Firmware ${device.firmware}`}>
      <span className="dot" />
      {device.model}
    </div>
  ) : (
    <div
      className="badge"
      title="Connect your Kobo with USB and tap Connect on its screen"
    >
      <span className="dot" />
      No device
    </div>
  );
}
