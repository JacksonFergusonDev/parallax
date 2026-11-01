import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  changeCollections,
  deviceCoverSrc,
  deviceBooks,
  ejectDevice,
  importFromDevice,
  listCollections,
  onImportProgress,
  removeFromDevice,
  tidyDevice,
  type Book,
  type Collection,
  type CollectionChange,
  type DeviceBook,
  type DeviceInfo,
} from "../api";
import {
  deviceColumns,
  filterDeviceBooks,
  shortDate,
  statusLabel,
  summarize,
  summarizeDeviceImport,
  type DeviceKey,
} from "../device";
import { applySort } from "../sort";
import { AboutKobo } from "./AboutKobo";
import { CollectionMenu } from "./CollectionMenu";
import { DeviceCard } from "./DeviceCard";
import { FolderImport } from "./FolderImport";
import { NameDialog } from "./NameDialog";
import { RulesDialog } from "./RulesDialog";
import {
  SortHeader,
  ViewControls,
  listTracks,
  useViewState,
} from "./ViewControls";

const DEVICE_KEYS = deviceColumns.map((c) => c.key);

interface Props {
  device: DeviceInfo;
  /** Increment after app operations that can change device metadata or covers. */
  revision: number;
  query: string;
  /** Library books by device ContentID, for covers. */
  libraryByContentId: Map<string, Book>;
  onChanged: () => void;
  /** Books were added to the library. */
  onLibraryChanged: () => void;
  onEjected: () => void;
}

type Dialog =
  | { kind: "new"; addSelected: boolean }
  | { kind: "rename"; name: string }
  | { kind: "folders" }
  | { kind: "rules" }
  | { kind: "about" }
  | null;

export function DeviceView({
  device,
  revision,
  query,
  libraryByContentId,
  onChanged,
  onLibraryChanged,
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
  const request = useRef({ generation: 0 });
  const { mode, setMode, sort, setSort } = useViewState<DeviceKey>(
    "device",
    DEVICE_KEYS,
    "grid",
  );

  const load = useCallback(() => {
    const state = request.current;
    const current = ++state.generation;
    return Promise.all([
      deviceBooks(device.mount),
      listCollections(device.mount),
    ])
      .then(([b, c]) => {
        if (current !== state.generation) return;
        setBooks(b);
        setCollections(c);
      })
      .catch((e) => {
        if (current === state.generation) {
          setNotice({ kind: "error", text: String(e) });
        }
      });
  }, [device.mount]);

  useEffect(() => {
    const state = request.current;
    load();
    return () => {
      // A refresh or disconnect supersedes any outstanding device read.
      state.generation++;
    };
  }, [load, revision]);

  const inShelf = useMemo(
    () =>
      (books ?? []).filter(
        (b) => shelf === null || b.collections.includes(shelf),
      ),
    [books, shelf],
  );
  const visible = useMemo(
    () => applySort(filterDeviceBooks(inShelf, query), sort, deviceColumns),
    [inShelf, query, sort],
  );
  const chosen = visible.filter((b) => selected.has(b.id));
  const removable = chosen.filter((b) => b.sideloaded);
  const importable = chosen.filter(
    (b) => b.sideloaded && !libraryByContentId.has(b.id),
  );

  const toggle = useCallback(
    (id: string, e: React.MouseEvent) => {
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
    },
    [visible],
  );

  /** Runs a device change with busy state, notices and a reload. */
  const run = async (action: () => Promise<string>) => {
    setBusy("working");
    setNotice(null);
    try {
      const text = await action();
      setNotice({ kind: "info", text });
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      // A failed batch may still have changed some books.
      await load();
      onChanged();
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

  /** Adds books to the library: the given ones, or every book when null. */
  const addToLibrary = (contentIds: string[] | null) =>
    run(async () => {
      const unlisten = await onImportProgress((p) =>
        setNotice({
          kind: "info",
          text: `Adding ${p.index} of ${p.total}: ${p.title}…`,
        }),
      );
      try {
        const outcomes = await importFromDevice(device.mount, contentIds);
        setSelected(new Set());
        onLibraryChanged();
        return summarizeDeviceImport(outcomes);
      } finally {
        unlisten();
      }
    });

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
              onClick={() => setDialog({ kind: "rules" })}
            >
              Automatic collections…
            </button>
            <button
              className="link"
              onClick={() => setDialog({ kind: "folders" })}
              disabled={busy !== null}
            >
              Folders → collections…
            </button>
            <button
              className="link"
              onClick={() => addToLibrary(null)}
              disabled={busy !== null}
              title="Copy every book you added yourself into your library, as plain EPUBs. Store books are skipped."
            >
              Add all to library
            </button>
            <button
              className="link"
              onClick={tidy}
              disabled={busy !== null}
              title="Delete hidden ._ files macOS leaves on the Kobo"
            >
              Remove macOS clutter
            </button>
            <button
              className="link"
              onClick={() => setDialog({ kind: "about" })}
            >
              About this Kobo…
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
            <ViewControls
              mode={mode}
              onMode={setMode}
              sort={sort}
              onSort={setSort}
              columns={deviceColumns}
            />
          </div>
          <div className="scroll">
            {books && visible.length === 0 ? (
              <p className="muted empty">
                {query
                  ? `No books match “${query}”.`
                  : shelf
                    ? "This collection is empty."
                    : "No books on this Kobo."}
              </p>
            ) : mode === "grid" ? (
              <div className="grid">
                {visible.map((b) => {
                  const lib = libraryByContentId.get(b.id);
                  return (
                    <DeviceCard
                      key={b.id}
                      book={b}
                      cover={deviceCoverSrc(b, lib)}
                      selected={selected.has(b.id)}
                      onSelect={toggle}
                    />
                  );
                })}
              </div>
            ) : (
              <div
                className="book-list"
                role="table"
                style={
                  {
                    "--tracks": listTracks(deviceColumns),
                  } as React.CSSProperties
                }
              >
                <SortHeader
                  columns={deviceColumns}
                  sort={sort}
                  onSort={setSort}
                />
                {visible.map((b) => {
                  const lib = libraryByContentId.get(b.id);
                  const src = deviceCoverSrc(b, lib);
                  return (
                    <button
                      key={b.id}
                      className={`list-row ${selected.has(b.id) ? "selected" : ""}`}
                      aria-pressed={selected.has(b.id)}
                      onClick={(e) => toggle(b.id, e)}
                    >
                      <span className="thumb">
                        {src ? <img src={src} alt="" loading="lazy" /> : null}
                      </span>
                      <span className="row-main">
                        <span className="row-title">{b.title}</span>
                        {b.collections.length > 0 && (
                          <span className="row-sub">
                            {b.collections.join(", ")}
                          </span>
                        )}
                      </span>
                      <span className="cell muted">
                        {b.author ?? "Unknown author"}
                      </span>
                      <span className="cell muted">
                        {b.series
                          ? `${b.series}${b.series_number ? ` #${b.series_number}` : ""}`
                          : ""}
                      </span>
                      <span className={`cell end status-${b.read_status}`}>
                        {statusLabel(b)}
                      </span>
                      <span className="cell end">{shortDate(b.last_read)}</span>
                      <span className="cell end muted">
                        {shortDate(b.added)}
                      </span>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
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
        {importable.length > 0 && (
          <button
            className="secondary"
            onClick={() => addToLibrary(ids(importable))}
            disabled={busy !== null}
            title="Copy these books into your library as plain EPUBs"
          >
            {`Add ${importable.length} to library`}
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
      {dialog?.kind === "rules" && (
        <RulesDialog
          mount={device.mount}
          onClose={() => setDialog(null)}
          onApplied={(n) => {
            setNotice({
              kind: "info",
              text: `Filed ${plural(n, "book")} into collections. Eject to see them on your Kobo.`,
            });
            load();
            onChanged();
          }}
        />
      )}
      {dialog?.kind === "about" && (
        <AboutKobo
          device={device}
          titles={new Map((books ?? []).map((b) => [b.id, b.title]))}
          onClose={() => setDialog(null)}
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
