import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  coverSrc,
  deviceBooks,
  ejectDevice,
  removeFromDevice,
  tidyDevice,
  type Book,
  type DeviceBook,
  type DeviceInfo,
} from "../api";
import {
  filterDeviceBooks,
  shortDate,
  statusLabel,
  summarize,
} from "../device";

interface Props {
  device: DeviceInfo;
  query: string;
  /** Library books by device ContentID, for covers. */
  libraryByContentId: Map<string, Book>;
  onChanged: () => void;
  onEjected: () => void;
}

export function DeviceView({
  device,
  query,
  libraryByContentId,
  onChanged,
  onEjected,
}: Props) {
  const [books, setBooks] = useState<DeviceBook[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<"removing" | "tidying" | "ejecting" | null>(
    null,
  );
  const [notice, setNotice] = useState<{
    kind: "error" | "info";
    text: string;
  } | null>(null);
  const anchor = useRef<string | null>(null);

  const load = useCallback(
    () =>
      deviceBooks(device.mount)
        .then(setBooks)
        .catch((e) => setNotice({ kind: "error", text: String(e) })),
    [device.mount],
  );

  useEffect(() => {
    load();
  }, [load]);

  const visible = useMemo(
    () => filterDeviceBooks(books ?? [], query),
    [books, query],
  );

  const toggle = (id: string, e: React.MouseEvent) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (e.shiftKey && anchor.current) {
        const ids = visible.map((b) => b.id);
        const [a, b] = [ids.indexOf(anchor.current), ids.indexOf(id)].sort(
          (x, y) => x - y,
        );
        if (a >= 0) ids.slice(a, b + 1).forEach((i) => next.add(i));
        return next;
      }
      anchor.current = id;
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const removable = visible.filter((b) => selected.has(b.id) && b.sideloaded);

  const remove = async () => {
    const n = removable.length;
    const ok = await ask(
      `Remove ${n} ${n === 1 ? "book" : "books"} from your Kobo? Reading progress and highlights for ${n === 1 ? "it" : "them"} are deleted too. A backup of the Kobo's database is made first.`,
      {
        title: "Remove from Kobo",
        kind: "warning",
        okLabel: "Remove",
        cancelLabel: "Cancel",
      },
    );
    if (!ok) return;
    setBusy("removing");
    setNotice(null);
    try {
      const outcomes = await removeFromDevice(
        device.mount,
        removable.map((b) => b.id),
      );
      const failed = outcomes.filter((o) => "Err" in o.result);
      setNotice(
        failed.length
          ? {
              kind: "error",
              text: `${failed.length} could not be removed: ${"Err" in failed[0].result ? failed[0].result.Err : ""}`,
            }
          : {
              kind: "info",
              text: `Removed ${outcomes.length}. Eject to let your Kobo refresh its library.`,
            },
      );
      setSelected(new Set());
      await load();
      onChanged();
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const tidy = async () => {
    setBusy("tidying");
    try {
      const n = await tidyDevice(device.mount);
      setNotice({
        kind: "info",
        text: n
          ? `Removed ${n} hidden macOS files.`
          : "No macOS clutter found.",
      });
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const eject = async () => {
    setBusy("ejecting");
    try {
      await ejectDevice(device.mount);
      onEjected();
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
      setBusy(null);
    }
  };

  return (
    <>
      <main className="content">
        <div className="device-toolbar">
          <span className="muted">
            {books ? summarize(books) : "Reading your Kobo…"}
          </span>
          <span className="spacer" />
          <button
            className="link"
            onClick={tidy}
            disabled={busy !== null}
            title="Delete hidden ._ files macOS leaves on the Kobo"
          >
            {busy === "tidying" ? "Cleaning…" : "Remove macOS clutter"}
          </button>
        </div>
        {books && visible.length === 0 ? (
          <p className="muted empty">
            {books.length
              ? `No books match “${query}”.`
              : "No books on this Kobo."}
          </p>
        ) : (
          <ul className="device-list">
            <li className="device-row header" aria-hidden>
              <span />
              <span>Title</span>
              <span>Status</span>
              <span>Last read</span>
              <span>On Kobo since</span>
            </li>
            {visible.map((b) => {
              const lib = libraryByContentId.get(b.id);
              const src = lib ? coverSrc(lib) : null;
              return (
                <li key={b.id}>
                  <button
                    className={`device-row ${selected.has(b.id) ? "selected" : ""}`}
                    onClick={(e) => toggle(b.id, e)}
                  >
                    <span className="thumb">
                      {src ? <img src={src} alt="" loading="lazy" /> : null}
                    </span>
                    <span className="row-main">
                      <span className="row-title">{b.title}</span>
                      <span className="row-sub">
                        {b.author ?? "Unknown author"}
                        {b.series
                          ? ` · ${b.series}${b.series_number ? ` #${b.series_number}` : ""}`
                          : ""}
                      </span>
                    </span>
                    <span className={`row-status status-${b.read_status}`}>
                      {statusLabel(b)}
                    </span>
                    <span className="row-date" title="Last read">
                      {shortDate(b.last_read)}
                    </span>
                    <span className="row-date muted" title="On device since">
                      {shortDate(b.added)}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </main>
      <footer className="actions">
        {notice && (
          <span className={`notice notice-${notice.kind}`}>{notice.text}</span>
        )}
        <span className="spacer" />
        <button className="secondary" onClick={eject} disabled={busy !== null}>
          {busy === "ejecting" ? "Ejecting…" : "Eject Kobo"}
        </button>
        <button
          className="primary danger-fill"
          onClick={remove}
          disabled={removable.length === 0 || busy !== null}
        >
          {busy === "removing"
            ? "Removing…"
            : removable.length
              ? `Remove ${removable.length} from Kobo`
              : "Remove from Kobo"}
        </button>
      </footer>
    </>
  );
}
