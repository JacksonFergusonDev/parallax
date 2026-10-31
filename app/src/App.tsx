import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  booksOnDevice,
  ejectDevice,
  importBooks,
  listBooks,
  listDevices,
  onSendProgress,
  removeBooks,
  sendBooks,
  updateBook,
  type Book,
  type DeviceInfo,
  type MetadataEdit,
} from "./api";
import {
  applyProgress,
  filterBooks,
  isBookFile,
  summarizeImport,
  type SendStatus,
} from "./library";
import { BookCard } from "./components/BookCard";
import { DeviceBadge } from "./components/DeviceBadge";
import { EditPanel } from "./components/EditPanel";
import "./styles.css";

const DEVICE_POLL_MS = 2000;

type Notice = { kind: "error" | "info"; text: string };

export default function App() {
  const [books, setBooks] = useState<Book[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [device, setDevice] = useState<DeviceInfo | null>(null);
  const [onDevice, setOnDevice] = useState<Set<number>>(new Set());
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [query, setQuery] = useState("");
  const [statuses, setStatuses] = useState<Record<number, SendStatus>>({});
  const [busy, setBusy] = useState<"importing" | "sending" | "ejecting" | null>(
    null,
  );
  const [dragging, setDragging] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);
  const batch = useRef<number[]>([]);
  const anchor = useRef<number | null>(null);

  const refresh = useCallback(async () => {
    try {
      setBooks(await listBooks());
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    listBooks()
      .then(setBooks)
      .catch((e) => setNotice({ kind: "error", text: String(e) }))
      .finally(() => setLoaded(true));
  }, []);

  // Poll for a connected Kobo. Cheap: it only stats /Volumes/*/.kobo/version.
  useEffect(() => {
    let alive = true;
    const poll = () =>
      listDevices()
        .then((devices) =>
          setDevice((prev) => {
            const next = devices[0] ?? null;
            return alive &&
              (prev?.mount !== next?.mount || prev?.serial !== next?.serial)
              ? next
              : prev;
          }),
        )
        .catch(() => alive && setDevice(null));
    poll();
    const timer = setInterval(poll, DEVICE_POLL_MS);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);

  const refreshOnDevice = useCallback(async (d: DeviceInfo | null) => {
    const ids = d ? await booksOnDevice(d.mount).catch(() => []) : [];
    setOnDevice(new Set(ids));
  }, []);

  useEffect(() => {
    let alive = true;
    const ids = device
      ? booksOnDevice(device.mount).catch(() => [])
      : Promise.resolve([]);
    ids.then((list) => alive && setOnDevice(new Set(list)));
    return () => {
      alive = false;
    };
  }, [device]);

  useEffect(() => {
    const unlisten = onSendProgress((p) =>
      setStatuses((s) => applyProgress(s, batch.current, p)),
    );
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const doImport = useCallback(
    async (paths: string[]) => {
      const files = paths.filter(isBookFile);
      if (files.length === 0) {
        setNotice({ kind: "error", text: "Only EPUB files can be added." });
        return;
      }
      setBusy("importing");
      try {
        const results = await importBooks(files);
        setNotice({ kind: "info", text: summarizeImport(results) });
        await refresh();
      } catch (e) {
        setNotice({ kind: "error", text: String(e) });
      } finally {
        setBusy(null);
      }
    },
    [refresh],
  );

  useEffect(() => {
    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      const { type } = event.payload;
      if (type === "over" || type === "enter") setDragging(true);
      else if (type === "leave") setDragging(false);
      else if (type === "drop") {
        setDragging(false);
        doImport(event.payload.paths);
      }
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, [doImport]);

  const visible = useMemo(() => filterBooks(books, query), [books, query]);
  const selectedBooks = useMemo(
    () => books.filter((b) => selected.has(b.id)),
    [books, selected],
  );
  const editing = selectedBooks.length === 1 ? selectedBooks[0] : null;

  const onSelect = useCallback(
    (id: number, e: React.MouseEvent) => {
      setSelected((prev) => {
        if (e.shiftKey && anchor.current !== null) {
          const ids = visible.map((b) => b.id);
          const [a, b] = [ids.indexOf(anchor.current), ids.indexOf(id)].sort(
            (x, y) => x - y,
          );
          if (a >= 0) return new Set([...prev, ...ids.slice(a, b + 1)]);
        }
        anchor.current = id;
        if (e.metaKey || e.ctrlKey) {
          const next = new Set(prev);
          if (next.has(id)) next.delete(id);
          else next.add(id);
          return next;
        }
        return prev.size === 1 && prev.has(id) ? new Set() : new Set([id]);
      });
    },
    [visible],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.target as HTMLElement).closest("input, textarea")) return;
      if (e.key === "Escape") setSelected(new Set());
      if ((e.metaKey || e.ctrlKey) && e.key === "a") {
        e.preventDefault();
        setSelected(new Set(visible.map((b) => b.id)));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [visible]);

  const chooseFiles = async () => {
    const picked = await open({
      multiple: true,
      filters: [{ name: "EPUB", extensions: ["epub"] }],
    });
    if (picked) doImport(Array.isArray(picked) ? picked : [picked]);
  };

  const save = async (id: number, edit: MetadataEdit) => {
    const updated = await updateBook(id, edit);
    setBooks((bs) => bs.map((b) => (b.id === id ? updated : b)));
  };

  const send = async () => {
    if (!device || selected.size === 0) return;
    const ids = visible.filter((b) => selected.has(b.id)).map((b) => b.id);
    batch.current = ids;
    setStatuses(Object.fromEntries(ids.map((id) => [id, { stage: "queued" }])));
    setBusy("sending");
    setNotice(null);
    try {
      const outcomes = await sendBooks(device.mount, ids);
      const failed = outcomes.filter((o) => "Err" in o.result).length;
      setNotice(
        failed
          ? {
              kind: "error",
              text: `${failed} of ${outcomes.length} could not be sent. Hover a book to see why.`,
            }
          : {
              kind: "info",
              text: "Sent. Eject your Kobo and unplug it to import the new books.",
            },
      );
      await refreshOnDevice(device);
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const remove = async () => {
    const n = selected.size;
    if (
      !window.confirm(
        `Remove ${n} ${n === 1 ? "book" : "books"} from your library? Copies on your Kobo stay.`,
      )
    )
      return;
    try {
      await removeBooks([...selected]);
      setSelected(new Set());
      await refresh();
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    }
  };

  const eject = async () => {
    if (!device) return;
    setBusy("ejecting");
    try {
      await ejectDevice(device.mount);
      setDevice(null);
      setStatuses({});
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

  const sentThisSession = Object.values(statuses).some(
    (s) => s.stage === "sent",
  );

  return (
    <div className={`app ${dragging ? "dragging" : ""}`}>
      <header className="topbar">
        <h1>Library</h1>
        <input
          className="search"
          type="search"
          placeholder="Search title, author, series, tag"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <DeviceBadge device={device} />
      </header>

      <div className="body">
        <main
          className="content"
          onClick={(e) =>
            e.target === e.currentTarget && setSelected(new Set())
          }
        >
          {loaded && books.length === 0 ? (
            <div className="dropzone">
              <p className="dropzone-title">Drop EPUB files here</p>
              <p className="muted">
                They're added to your library, ready to edit and send to your
                Kobo.
              </p>
              <button className="secondary" onClick={chooseFiles}>
                Choose files…
              </button>
            </div>
          ) : visible.length === 0 && loaded ? (
            <p className="muted empty">No books match “{query}”.</p>
          ) : (
            <div className="grid">
              {visible.map((b) => (
                <BookCard
                  key={b.id}
                  book={b}
                  selected={selected.has(b.id)}
                  onDevice={onDevice.has(b.id)}
                  status={statuses[b.id]}
                  onSelect={onSelect}
                />
              ))}
            </div>
          )}
        </main>
        {editing && (
          <EditPanel
            book={editing}
            onSave={save}
            onClose={() => setSelected(new Set())}
          />
        )}
      </div>

      <footer className="actions">
        <button
          className="secondary"
          onClick={chooseFiles}
          disabled={busy !== null}
        >
          {busy === "importing" ? "Adding…" : "Add books…"}
        </button>
        {notice && (
          <span className={`notice notice-${notice.kind}`}>{notice.text}</span>
        )}
        <span className="spacer" />
        {selected.size > 0 && (
          <button
            className="secondary danger"
            onClick={remove}
            disabled={busy !== null}
          >
            Remove
          </button>
        )}
        {device && sentThisSession && (
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
          disabled={!device || selected.size === 0 || busy !== null}
          title={device ? undefined : "Connect your Kobo to send books"}
        >
          {busy === "sending"
            ? "Sending…"
            : selected.size > 0
              ? `Send ${selected.size} to Kobo`
              : "Send to Kobo"}
        </button>
      </footer>
    </div>
  );
}
