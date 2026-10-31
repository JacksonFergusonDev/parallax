import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ask, open } from "@tauri-apps/plugin-dialog";
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
  updateBooks,
  type Book,
  type DeviceInfo,
  type MetadataEdit,
  type Collection,
  type OnDevice,
  listCollections,
  reconcileDevice,
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
import { DeviceView } from "./components/DeviceView";
import { EditPanel } from "./components/EditPanel";
import "./styles.css";

const DEVICE_POLL_MS = 2000;
const NEW_COLLECTION = "\u0000new";

type Notice = { kind: "error" | "info"; text: string };

export default function App() {
  const [books, setBooks] = useState<Book[]>([]);
  const [loaded, setLoaded] = useState(false);
  const [device, setDevice] = useState<DeviceInfo | null>(null);
  const [onDevice, setOnDevice] = useState<Map<number, OnDevice>>(new Map());
  const [view, setView] = useState<"library" | "device">("library");
  const [collections, setCollections] = useState<Collection[]>([]);
  /** Collection to put sent books in: "" none, NEW_COLLECTION, or a name. */
  const [sendCollection, setSendCollection] = useState("");
  const [newCollection, setNewCollection] = useState("");
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [query, setQuery] = useState("");
  const [statuses, setStatuses] = useState<Record<number, SendStatus>>({});
  const [busy, setBusy] = useState<
    "importing" | "sending" | "updating" | "ejecting" | null
  >(null);
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
    const list = d ? await booksOnDevice(d.mount).catch(() => []) : [];
    setOnDevice(new Map(list.map((o) => [o.book_id, o])));
  }, []);

  // When a Kobo connects: learn which library books are on it, list its
  // collections, and apply series to books it has imported since they were
  // sent (Nickel ignores series in sideloaded EPUBs).
  useEffect(() => {
    let alive = true;
    const ids = device
      ? booksOnDevice(device.mount).catch(() => [])
      : Promise.resolve([]);
    ids.then(
      (list) => alive && setOnDevice(new Map(list.map((o) => [o.book_id, o]))),
    );
    const shelves = device
      ? listCollections(device.mount).catch(() => [])
      : Promise.resolve([]);
    shelves.then((list) => alive && setCollections(list));
    if (device) {
      reconcileDevice(device.mount)
        .then(
          (n) =>
            alive &&
            n > 0 &&
            setNotice({
              kind: "info",
              text: `Added series details to ${n} ${n === 1 ? "book" : "books"} on your Kobo.`,
            }),
        )
        .catch((e) => alive && setNotice({ kind: "error", text: String(e) }));
    }
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
  const toSend = visible.filter(
    (b) => selected.has(b.id) && !onDevice.has(b.id),
  );
  const toUpdate = visible.filter(
    (b) => selected.has(b.id) && onDevice.get(b.id)?.stale,
  );
  const libraryByContentId = useMemo(() => {
    const byId = new Map(books.map((b) => [b.id, b]));
    const map = new Map<string, Book>();
    for (const o of onDevice.values()) {
      const b = byId.get(o.book_id);
      if (b) map.set(o.content_id, b);
    }
    return map;
  }, [books, onDevice]);
  // The Kobo tab only exists while one is connected.
  const activeView = device ? view : "library";

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

  const transfer = async (kind: "sending" | "updating", ids: number[]) => {
    if (!device || ids.length === 0) return;
    batch.current = ids;
    setStatuses(Object.fromEntries(ids.map((id) => [id, { stage: "queued" }])));
    setBusy(kind);
    setNotice(null);
    try {
      const collection =
        sendCollection === NEW_COLLECTION
          ? newCollection.trim() || null
          : sendCollection || null;
      const outcomes =
        kind === "sending"
          ? await sendBooks(device.mount, ids, collection)
          : await updateBooks(device.mount, ids);
      if (kind === "sending" && collection) {
        setCollections(await listCollections(device.mount).catch(() => []));
        setSendCollection(collection);
        setNewCollection("");
      }
      const failed = outcomes.filter((o) => "Err" in o.result).length;
      setNotice(
        failed
          ? {
              kind: "error",
              text: `${failed} of ${outcomes.length} failed. Hover a book to see why.`,
            }
          : {
              kind: "info",
              text:
                kind === "sending"
                  ? "Sent. Eject your Kobo and unplug it to import the new books."
                  : "Updated. Eject your Kobo to see the changes.",
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
    const ok = await ask(
      `Remove ${n} ${n === 1 ? "book" : "books"} from your library? Copies on your Kobo stay.`,
      {
        title: "Remove from library",
        kind: "warning",
        okLabel: "Remove",
        cancelLabel: "Cancel",
      },
    );
    if (!ok) return;
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
        {device ? (
          <div className="tabs" role="tablist">
            <button
              role="tab"
              className={activeView === "library" ? "active" : ""}
              onClick={() => setView("library")}
            >
              Library
            </button>
            <button
              role="tab"
              className={activeView === "device" ? "active" : ""}
              onClick={() => {
                setSelected(new Set());
                setView("device");
              }}
            >
              On Kobo
            </button>
          </div>
        ) : (
          <h1>Library</h1>
        )}
        <input
          className="search"
          type="search"
          placeholder="Search title, author, series, tag"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <DeviceBadge device={device} />
      </header>

      {activeView === "device" && device ? (
        <DeviceView
          key={device.mount}
          device={device}
          query={query}
          libraryByContentId={libraryByContentId}
          onChanged={() => {
            refreshOnDevice(device);
            listCollections(device.mount)
              .then(setCollections)
              .catch(() => {});
          }}
          onEjected={() => {
            setDevice(null);
            setStatuses({});
            setNotice({
              kind: "info",
              text: "Ejected. Unplug your Kobo; it will refresh its library.",
            });
          }}
        />
      ) : (
        <>
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
                    They're added to your library, ready to edit and send to
                    your Kobo.
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
                      deviceState={
                        onDevice.get(b.id)?.stale
                          ? "stale"
                          : onDevice.has(b.id)
                            ? "on"
                            : undefined
                      }
                      status={statuses[b.id]}
                      onSelect={onSelect}
                    />
                  ))}
                </div>
              )}
            </main>
            {editing && (
              <EditPanel
                key={editing.id}
                book={editing}
                onSave={save}
                onChanged={(updated) =>
                  setBooks((bs) =>
                    bs.map((b) => (b.id === updated.id ? updated : b)),
                  )
                }
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
              <span className={`notice notice-${notice.kind}`}>
                {notice.text}
              </span>
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
            {toUpdate.length > 0 && (
              <button
                className="secondary"
                onClick={() =>
                  transfer(
                    "updating",
                    toUpdate.map((b) => b.id),
                  )
                }
                disabled={!device || busy !== null}
                title="Replace the copies on your Kobo with your edits, keeping reading progress"
              >
                {busy === "updating"
                  ? "Updating…"
                  : `Update ${toUpdate.length} on Kobo`}
              </button>
            )}
            {device && toSend.length > 0 && (
              <>
                <select
                  className="collection-select"
                  aria-label="Collection for sent books"
                  value={sendCollection}
                  onChange={(e) => setSendCollection(e.target.value)}
                  disabled={busy !== null}
                >
                  <option value="">No collection</option>
                  {collections.map((c) => (
                    <option key={c.name} value={c.name}>
                      Into “{c.name}”
                    </option>
                  ))}
                  <option value={NEW_COLLECTION}>New collection…</option>
                </select>
                {sendCollection === NEW_COLLECTION && (
                  <input
                    className="new-collection"
                    placeholder="Collection name"
                    value={newCollection}
                    onChange={(e) => setNewCollection(e.target.value)}
                    autoFocus
                  />
                )}
              </>
            )}
            <button
              className="primary"
              onClick={() =>
                transfer(
                  "sending",
                  toSend.map((b) => b.id),
                )
              }
              disabled={!device || toSend.length === 0 || busy !== null}
              title={device ? undefined : "Connect your Kobo to send books"}
            >
              {busy === "sending"
                ? "Sending…"
                : toSend.length > 0
                  ? `Send ${toSend.length} to Kobo`
                  : "Send to Kobo"}
            </button>
          </footer>
        </>
      )}
    </div>
  );
}
