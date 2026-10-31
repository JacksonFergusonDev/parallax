import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  changeCollections,
  coverSrc,
  deviceBooks,
  ejectDevice,
  listCollections,
  removeFromDevice,
  tidyDevice,
  type Book,
  type Collection,
  type CollectionChange,
  type DeviceBook,
  type DeviceInfo,
} from "../api";
import {
  filterDeviceBooks,
  shortDate,
  statusLabel,
  summarize,
} from "../device";
import { CollectionMenu } from "./CollectionMenu";
import { FolderImport } from "./FolderImport";
import { NameDialog } from "./NameDialog";

interface Props {
  device: DeviceInfo;
  query: string;
  /** Library books by device ContentID, for covers. */
  libraryByContentId: Map<string, Book>;
  onChanged: () => void;
  onEjected: () => void;
}

type Dialog =
  | { kind: "new"; addSelected: boolean }
  | { kind: "rename"; name: string }
  | { kind: "folders" }
  | null;

export function DeviceView({
  device,
  query,
  libraryByContentId,
  onChanged,
  onEjected,
}: Props) {
  const [books, setBooks] = useState<DeviceBook[] | null>(null);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [shelf, setShelf] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<"working" | "ejecting" | null>(null);
  const [dialog, setDialog] = useState<Dialog>(null);
  const [notice, setNotice] = useState<{
    kind: "error" | "info";
    text: string;
  } | null>(null);
  const anchor = useRef<string | null>(null);

  const load = useCallback(
    () =>
      Promise.all([deviceBooks(device.mount), listCollections(device.mount)])
        .then(([b, c]) => {
          setBooks(b);
          setCollections(c);
        })
        .catch((e) => setNotice({ kind: "error", text: String(e) })),
    [device.mount],
  );

  useEffect(() => {
    load();
  }, [load]);

  const inShelf = useMemo(
    () =>
      (books ?? []).filter(
        (b) => shelf === null || b.collections.includes(shelf),
      ),
    [books, shelf],
  );
  const visible = useMemo(
    () => filterDeviceBooks(inShelf, query),
    [inShelf, query],
  );
  const chosen = visible.filter((b) => selected.has(b.id));
  const removable = chosen.filter((b) => b.sideloaded);

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

  /** Runs a device change with busy state, notices and a reload. */
  const run = async (action: () => Promise<string>) => {
    setBusy("working");
    setNotice(null);
    try {
      const text = await action();
      setNotice({ kind: "info", text });
      await load();
      onChanged();
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const change = (changes: CollectionChange[], done: string) =>
    run(async () => {
      await changeCollections(device.mount, changes);
      return done;
    });

  const ids = (list: DeviceBook[]) => list.map((b) => b.id);
  const plural = (n: number, word: string) =>
    `${n} ${word}${n === 1 ? "" : "s"}`;

  const addTo = (name: string) => {
    change(
      [{ kind: "add", name, content_ids: ids(chosen) }],
      `Added ${plural(chosen.length, "book")} to “${name}”.`,
    );
    setSelected(new Set());
  };

  const removeFromShelf = () => {
    if (!shelf) return;
    change(
      [{ kind: "remove", name: shelf, content_ids: ids(chosen) }],
      `Removed ${plural(chosen.length, "book")} from “${shelf}”. They're still on your Kobo.`,
    );
    setSelected(new Set());
  };

  const deleteShelf = async (name: string) => {
    const ok = await ask(
      `Delete the collection “${name}”? The books stay on your Kobo.`,
      {
        title: "Delete collection",
        kind: "warning",
        okLabel: "Delete",
        cancelLabel: "Cancel",
      },
    );
    if (!ok) return;
    setShelf(null);
    change([{ kind: "delete", name }], `Deleted “${name}”.`);
  };

  const removeBooks = async () => {
    const n = removable.length;
    const ok = await ask(
      `Remove ${plural(n, "book")} from your Kobo? Reading progress and highlights are deleted too. A backup of the Kobo's database is made first.`,
      {
        title: "Remove from Kobo",
        kind: "warning",
        okLabel: "Remove",
        cancelLabel: "Cancel",
      },
    );
    if (!ok) return;
    run(async () => {
      const outcomes = await removeFromDevice(device.mount, ids(removable));
      setSelected(new Set());
      const failed = outcomes.filter((o) => "Err" in o.result);
      if (failed.length) {
        const first = failed[0].result;
        throw new Error(
          `${failed.length} could not be removed: ${"Err" in first ? first.Err : ""}`,
        );
      }
      return `Removed ${plural(outcomes.length, "book")}. Eject to let your Kobo refresh its library.`;
    });
  };

  const tidy = () =>
    run(async () => {
      const n = await tidyDevice(device.mount);
      return n
        ? `Removed ${plural(n, "hidden macOS file")}.`
        : "No macOS clutter found.";
    });

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
      <div className="body">
        <nav className="sidebar">
          <button
            className={`shelf ${shelf === null ? "active" : ""}`}
            onClick={() => setShelf(null)}
          >
            All books
            <span className="muted">{books?.length ?? ""}</span>
          </button>
          <div className="sidebar-heading">
            Collections
            <button
              className="icon"
              title="New collection"
              onClick={() => setDialog({ kind: "new", addSelected: false })}
            >
              +
            </button>
          </div>
          {collections.length === 0 && (
            <p className="muted sidebar-empty">No collections yet.</p>
          )}
          {collections.map((c) => (
            <button
              key={c.name}
              className={`shelf ${shelf === c.name ? "active" : ""}`}
              onClick={() => {
                setShelf(c.name);
                setSelected(new Set());
              }}
            >
              {c.name}
              <span className="muted">{c.book_count}</span>
            </button>
          ))}
          <div className="sidebar-foot">
            <button
              className="link"
              onClick={() => setDialog({ kind: "folders" })}
              disabled={busy !== null}
            >
              Folders → collections…
            </button>
            <button
              className="link"
              onClick={tidy}
              disabled={busy !== null}
              title="Delete hidden ._ files macOS leaves on the Kobo"
            >
              Remove macOS clutter
            </button>
          </div>
        </nav>

        <main className="content">
          <div className="device-toolbar">
            {shelf ? (
              <>
                <strong>{shelf}</strong>
                <button
                  className="link"
                  onClick={() => setDialog({ kind: "rename", name: shelf })}
                >
                  Rename
                </button>
                <button
                  className="link danger"
                  onClick={() => deleteShelf(shelf)}
                >
                  Delete
                </button>
              </>
            ) : (
              <span className="muted">
                {books ? summarize(books) : "Reading your Kobo…"}
              </span>
            )}
            <span className="spacer" />
            {chosen.length > 0 && (
              <span className="muted">{chosen.length} selected</span>
            )}
          </div>
          {books && visible.length === 0 ? (
            <p className="muted empty">
              {query
                ? `No books match “${query}”.`
                : shelf
                  ? "This collection is empty."
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
                          {b.collections.length > 0 && (
                            <span className="chips">
                              {" "}
                              · {b.collections.join(", ")}
                            </span>
                          )}
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
      </div>

      <footer className="actions">
        {notice && (
          <span className={`notice notice-${notice.kind}`}>{notice.text}</span>
        )}
        <span className="spacer" />
        <button className="secondary" onClick={eject} disabled={busy !== null}>
          {busy === "ejecting" ? "Ejecting…" : "Eject Kobo"}
        </button>
        {shelf && (
          <button
            className="secondary"
            onClick={removeFromShelf}
            disabled={chosen.length === 0 || busy !== null}
          >
            Remove from collection
          </button>
        )}
        <CollectionMenu
          label="Add to collection"
          collections={collections.filter((c) => c.name !== shelf)}
          disabled={chosen.length === 0 || busy !== null}
          onPick={addTo}
          onNew={() => setDialog({ kind: "new", addSelected: true })}
        />
        <button
          className="primary danger-fill"
          onClick={removeBooks}
          disabled={removable.length === 0 || busy !== null}
        >
          {removable.length
            ? `Remove ${removable.length} from Kobo`
            : "Remove from Kobo"}
        </button>
      </footer>

      {dialog?.kind === "new" && (
        <NameDialog
          title={
            dialog.addSelected
              ? `New collection with ${plural(chosen.length, "book")}`
              : "New collection"
          }
          confirmLabel="Create"
          onClose={() => setDialog(null)}
          onSubmit={async (name) => {
            const changes: CollectionChange[] = dialog.addSelected
              ? [{ kind: "add", name, content_ids: ids(chosen) }]
              : [{ kind: "create", name }];
            await changeCollections(device.mount, changes);
            setSelected(new Set());
            setNotice({ kind: "info", text: `Created “${name}”.` });
            await load();
            onChanged();
          }}
        />
      )}
      {dialog?.kind === "rename" && (
        <NameDialog
          title="Rename collection"
          initial={dialog.name}
          confirmLabel="Rename"
          onClose={() => setDialog(null)}
          onSubmit={async (name) => {
            await changeCollections(device.mount, [
              { kind: "rename", from: dialog.name, to: name },
            ]);
            setShelf(name);
            await load();
            onChanged();
          }}
        />
      )}
      {dialog?.kind === "folders" && (
        <FolderImport
          mount={device.mount}
          onClose={() => setDialog(null)}
          onDone={(n) => {
            setNotice({
              kind: "info",
              text: `Created ${plural(n, "collection")}. Eject to see them on your Kobo.`,
            });
            load();
            onChanged();
          }}
        />
      )}
    </>
  );
}
