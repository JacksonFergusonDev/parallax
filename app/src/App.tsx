import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  BookOpen,
  SlidersHorizontal,
  Eject,
  LibraryBig,
  Plus,
  RefreshCw,
  Send,
  Tablet,
  Trash2,
} from "lucide-react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { ask, open } from "@tauri-apps/plugin-dialog";
import {
  booksOnDevice,
  changeCollections,
  coverSrc,
  deviceBooks,
  dismissDuplicates,
  ejectDevice,
  libraryDuplicates,
  getSetting,
  importBooks,
  reviewBooks,
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
  upscaleAvailable,
} from "./api";
import {
  applyProgress,
  defaultOrder,
  filterBooks,
  libraryColumns,
  type DeviceState,
  type LibraryKey,
  BOOK_EXTENSIONS,
  isBookFile,
  summarizeImport,
  type SendStatus,
} from "./library";
import {
  filterLibrary,
  isFiltered,
  libraryFacets,
  NO_FILTER,
  withoutKobo,
  type BookFacts,
  type LibraryFilter,
} from "./libraryFilter";
import { clickSelect, SELECTION_HINT } from "./selection";
import { BookCard } from "./components/BookCard";
import { CalibreImport } from "./components/CalibreImport";
import { ReadingView } from "./components/ReadingView";
import { ShelfArt } from "./components/ShelfArt";
import { TidyGenres } from "./components/TidyGenres";
import { CollectionMenu } from "./components/CollectionMenu";
import { NameDialog } from "./components/NameDialog";
import { DeviceBadge } from "./components/DeviceBadge";
import { DeviceView } from "./components/DeviceView";
import { EnhanceReview } from "./components/EnhanceReview";
import { ImportReview, REVIEW_SETTING } from "./components/ImportReview";
import { LibraryCheck } from "./components/LibraryCheck";
import { RevertChanges, type RevertScope } from "./components/RevertChanges";
import {
  DuplicateReview,
  type ReviewGroup,
} from "./components/DuplicateReview";
import { ActionMenu } from "./components/ActionMenu";
import { EditPanel } from "./components/EditPanel";
import { LibraryFilters } from "./components/LibraryFilters";
import { usePanelBook } from "./panelBook";
import { UpscaleReview } from "./components/UpscaleReview";
import {
  colorColumn,
  generatedColor,
  useCoverColors,
  useSettledSort,
} from "./coverColor";
import { LibraryRow } from "./components/LibraryRow";
import {
  SortHeader,
  ViewControls,
  listTracks,
  useViewState,
} from "./components/ViewControls";
import { applySort } from "./sort";
import { libraryReviewGroups } from "./duplicates";
import {
  addedIds,
  anyBetterCover,
  failedFiles,
  flaggedCount,
  needsReview,
  plural,
  sendableIds,
  type FailedFile,
  type ReviewBatch,
} from "./importReview";
import { usePresentation } from "./presentation";
import { rememberView, usePreferences, type View } from "./preferences";
import "./styles.css";

const DEVICE_POLL_MS = 2000;
const LIBRARY_KEYS = [
  ...libraryColumns(() => undefined).map((c) => c.key),
  "color" as const,
];
const NEW_COLLECTION = "\u0000new";

type Notice = { kind: "error" | "info"; text: string };

const none = new Set<number>();

export default function App() {
  const [books, setBooks] = useState<Book[]>([]);
  const [loaded, setLoaded] = useState(false);
  const appRef = useRef<HTMLDivElement>(null);
  const [detected, setDetected] = useState(false);
  const [view, setView] = useState<View>("library");
  // Opens on the view Settings says, once it's known.
  const themed = usePreferences(setView);
  usePresentation(appRef, loaded && detected && themed);
  const [device, setDevice] = useState<DeviceInfo | null>(null);
  const [onDevice, setOnDevice] = useState<Map<number, OnDevice>>(new Map());
  const shownOnce = useRef(false);
  useEffect(() => {
    // The first render is the default view, not one the user chose.
    if (!shownOnce.current) {
      shownOnce.current = true;
      return;
    }
    rememberView(view);
  }, [view]);
  const [collections, setCollections] = useState<Collection[]>([]);
  const [deviceRevision, setDeviceRevision] = useState(0);
  /** Collection to put sent books in: "" none, NEW_COLLECTION, or a name. */
  const [sendCollection, setSendCollection] = useState("");
  const [newCollection, setNewCollection] = useState("");
  /** Naming a new collection for selected books already on the Kobo. */
  const [naming, setNaming] = useState(false);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<LibraryFilter>(NO_FILTER);
  /** Collections of each book on the Kobo, by content id. */
  const [shelvesByContentId, setShelvesByContentId] = useState<
    Map<string, string[]>
  >(new Map());
  const [aboutOpen, setAboutOpen] = useState(false);
  const { mode, setMode, sort, setSort } = useViewState<LibraryKey>(
    "library",
    LIBRARY_KEYS,
    "grid",
  );
  const searchRef = useRef<HTMLInputElement>(null);
  const [statuses, setStatuses] = useState<Record<number, SendStatus>>({});
  const [busy, setBusy] = useState<
    "importing" | "sending" | "updating" | "ejecting" | null
  >(null);
  const [dragging, setDragging] = useState(false);
  const [duplicates, setDuplicates] = useState<Promise<ReviewGroup[]> | null>(
    null,
  );
  /** Books being looked up for series and genres. */
  const [enhancing, setEnhancing] = useState<number[] | null>(null);
  const [checking, setChecking] = useState(false);
  const [reverting, setReverting] = useState<RevertScope | null>(null);
  const [upscaling, setUpscaling] = useState<number[] | null>(null);
  const [canUpscale, setCanUpscale] = useState(false);
  useEffect(() => {
    upscaleAvailable()
      .then(setCanUpscale)
      .catch(() => {});
  }, []);
  const [fromCalibre, setFromCalibre] = useState(false);
  const [tidying, setTidying] = useState(false);
  /** Bumped when an enhance saves, so an open edit form reloads. */
  const [enhanced, setEnhanced] = useState(0);
  const [notice, setNotice] = useState<Notice | null>(null);
  /** Added books waiting for review; the first one is open. */
  const [reviews, setReviews] = useState<ReviewBatch[]>([]);
  /** Too many books to open a review for: it's offered instead. */
  const [reviewOffer, setReviewOffer] = useState<ReviewBatch | null>(null);
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

  // Poll for a connected Kobo without queuing work behind slow USB reads.
  useEffect(() => {
    let alive = true;
    let pending = false;
    const poll = () => {
      if (pending || !alive) return;
      pending = true;
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
        .catch(() => alive && setDevice(null))
        .finally(() => {
          pending = false;
          if (alive) setDetected(true);
        });
    };
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
        .then(({ series, grouped }) => {
          const parts = [];
          if (series)
            parts.push(
              `added series details to ${series} ${series === 1 ? "book" : "books"}`,
            );
          if (grouped)
            parts.push(
              `filed ${grouped} ${grouped === 1 ? "book" : "books"} into collections`,
            );
          if (alive && parts.length) {
            setDeviceRevision((revision) => revision + 1);
            listCollections(device.mount)
              .then((list) => alive && setCollections(list))
              .catch(() => {});
            const text = parts.join(" and ");
            setNotice({ kind: "info", text: `On your Kobo: ${text}.` });
          }
        })
        .catch((e) => {
          if (alive) {
            setDeviceRevision((revision) => revision + 1);
            setNotice({ kind: "error", text: String(e) });
          }
        });
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

  /** The connected Kobo, if books can be sent to it. */
  const writable = device?.read_only === null ? device : null;
  const sendTo = useRef(writable);
  useEffect(() => {
    sendTo.current = writable;
  });

  /** Sends books from the review, showing progress on them as usual. */
  const sendReviewed = async (ids: number[]) => {
    if (!writable) return [];
    batch.current = ids;
    setStatuses(Object.fromEntries(ids.map((id) => [id, { stage: "queued" }])));
    setBusy("sending");
    try {
      const outcomes = await sendBooks(writable.mount, ids);
      await refreshOnDevice(writable);
      return outcomes;
    } finally {
      setBusy(null);
    }
  };

  /** Looks over new books and raises what's worth fixing, if anything. */
  const reviewAdded = useCallback(
    async (
      ids: number[],
      failed: FailedFile[],
      from: "files" | "calibre" | "kobo" = "files",
    ) => {
      if (ids.length === 0 && failed.length === 0) return;
      try {
        if ((await getSetting(REVIEW_SETTING)) === "off") return;
        const found = await reviewBooks(ids);
        const review = { ...found, bulk: found.bulk || from === "calibre" };
        const added = { ids, failed, review, fromKobo: from === "kobo" };
        const offerSending =
          sendTo.current !== null &&
          sendableIds(added, none, () => false).length > 0;
        if (review.bulk) {
          if (flaggedCount(added) > 0) setReviewOffer(added);
        } else if (
          needsReview(added) ||
          offerSending ||
          (await anyBetterCover(review))
        ) {
          setReviews((queue) => [...queue, added]);
        }
      } catch (e) {
        setNotice({
          kind: "error",
          text: `Couldn't check the new books: ${String(e)}`,
        });
      }
    },
    [],
  );

  const doImport = useCallback(
    async (paths: string[]) => {
      const files = paths.filter(isBookFile);
      if (files.length === 0) {
        setNotice({
          kind: "error",
          text: "Only EPUB and Kindle books can be added.",
        });
        return;
      }
      setBusy("importing");
      try {
        const results = await importBooks(files);
        setNotice({ kind: "info", text: summarizeImport(results) });
        await refresh();
        void reviewAdded(addedIds(results), failedFiles(results));
      } catch (e) {
        setNotice({ kind: "error", text: String(e) });
      } finally {
        setBusy(null);
      }
    },
    [refresh, reviewAdded],
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

  const deviceState = useCallback(
    (b: Book): DeviceState =>
      onDevice.get(b.id)?.stale
        ? "stale"
        : onDevice.has(b.id)
          ? "on"
          : undefined,
    [onDevice],
  );
  // Read again whenever what's on the Kobo or its collections change.
  useEffect(() => {
    if (!device) return;
    let alive = true;
    deviceBooks(device.mount)
      .catch(() => [])
      .then(
        (list) =>
          alive &&
          setShelvesByContentId(
            new Map(list.map((b) => [b.id, b.collections])),
          ),
      );
    return () => {
      alive = false;
    };
  }, [device, onDevice, collections, deviceRevision]);
  const facts = useMemo<BookFacts>(
    () => ({
      deviceState,
      collectionsOf: (b) => {
        const contentId = onDevice.get(b.id)?.content_id;
        return (contentId && shelvesByContentId.get(contentId)) || [];
      },
    }),
    [deviceState, onDevice, shelvesByContentId],
  );
  const columns = useMemo(() => libraryColumns(deviceState), [deviceState]);
  const covers = useMemo(
    () =>
      books.flatMap((b) => {
        const key = coverSrc(b);
        return key && b.cover ? [{ key, path: b.cover }] : [];
      }),
    [books],
  );
  const { colorOf, loading: colorsLoading } = useCoverColors(
    sort?.key === "color",
    covers,
  );
  const sortColumns = useMemo(
    () => [
      ...columns,
      colorColumn<Book, LibraryKey>("color", (b) => {
        const key = coverSrc(b);
        return key ? colorOf(key) : generatedColor(b.title);
      }),
    ],
    [columns, colorOf],
  );
  const shownSort = useSettledSort(sort, colorsLoading);
  const searched = useMemo(() => filterBooks(books, query), [books, query]);
  const shownFilter = device ? filter : withoutKobo(filter);
  const facets = useMemo(
    () => libraryFacets(searched, shownFilter, facts),
    [searched, shownFilter, facts],
  );
  const visible = useMemo(
    () =>
      applySort(
        defaultOrder(filterLibrary(searched, shownFilter, facts)),
        shownSort,
        sortColumns,
      ),
    [searched, shownFilter, facts, shownSort, sortColumns],
  );
  const changeFilter = (next: LibraryFilter) => {
    setFilter(next);
    // Remove and Send act on the selection, so it mustn't hold hidden books.
    const shown = new Set(
      filterLibrary(searched, next, facts).map((b) => b.id),
    );
    setSelected((prev) => new Set([...prev].filter((id) => shown.has(id))));
  };
  const selectedBooks = useMemo(
    () => books.filter((b) => selected.has(b.id)),
    [books, selected],
  );
  const panel = usePanelBook(
    selectedBooks.length === 1 ? selectedBooks[0] : null,
  );
  const toSend = visible.filter(
    (b) => selected.has(b.id) && !onDevice.has(b.id),
  );
  const toUpdate = visible.filter(
    (b) => selected.has(b.id) && onDevice.get(b.id)?.stale,
  );
  const alreadyOnKobo = visible.filter(
    (b) => selected.has(b.id) && onDevice.has(b.id),
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
  const activeView = !device && view === "device" ? "library" : view;

  const onSelect = useCallback(
    (id: number, e: React.MouseEvent) =>
      setSelected((prev) =>
        clickSelect(
          prev,
          id,
          visible.map((b) => b.id),
          anchor,
          e,
        ),
      ),
    [visible],
  );

  const chooseFiles = useCallback(async () => {
    const picked = await open({
      multiple: true,
      filters: [{ name: "Books", extensions: BOOK_EXTENSIONS }],
    });
    if (picked) doImport(Array.isArray(picked) ? picked : [picked]);
  }, [doImport]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      // App-wide shortcuts work even while typing.
      if (mod && e.key === "f") {
        e.preventDefault();
        searchRef.current?.focus();
        searchRef.current?.select();
        return;
      }
      if (mod && e.key === "o") {
        e.preventDefault();
        chooseFiles();
        return;
      }
      if (mod && (e.key === "1" || e.key === "2" || e.key === "3")) {
        e.preventDefault();
        setSelected(new Set());
        setView(
          e.key === "1" ? "library" : e.key === "2" ? "device" : "reading",
        );
        return;
      }
      const typing = (e.target as HTMLElement).closest(
        "input, textarea, select",
      );
      if (typing) {
        if (e.key === "Escape") (e.target as HTMLElement).blur();
        return;
      }
      if (e.key === "Escape") setSelected(new Set());
      if (mod && e.key === "a") {
        e.preventDefault();
        setSelected(new Set(visible.map((b) => b.id)));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [visible, chooseFiles]);

  const save = async (id: number, edit: MetadataEdit) => {
    const updated = await updateBook(id, edit);
    setBooks((bs) => bs.map((b) => (b.id === id ? updated : b)));
    await refreshOnDevice(device);
  };

  const transfer = async (kind: "sending" | "updating", ids: number[]) => {
    if (!device || ids.length === 0) return;
    let thenEject = false;
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
      if (kind === "sending" && collection && alreadyOnKobo.length > 0) {
        // Selected books that were already on the Kobo join the collection too.
        await changeCollections(device.mount, [
          {
            kind: "add",
            name: collection,
            content_ids: alreadyOnKobo.map(
              (b) => onDevice.get(b.id)!.content_id,
            ),
          },
        ]);
      }
      if (kind === "sending" && collection) {
        setCollections(await listCollections(device.mount).catch(() => []));
        setSendCollection(collection);
        setNewCollection("");
      }
      const failed = outcomes.filter((o) => "Err" in o.result).length;
      thenEject =
        kind === "sending" &&
        failed === 0 &&
        (await getSetting("eject_after_send").catch(() => null)) === "on";
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
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      await refreshOnDevice(device);
      // Even a partially failed transfer may have changed device metadata.
      setDeviceRevision((revision) => revision + 1);
      listCollections(device.mount)
        .then(setCollections)
        .catch(() => {});
      setBusy(null);
    }
    if (thenEject) await eject();
  };

  /** Puts selected books that are already on the Kobo into a collection. */
  const addToCollection = async (name: string) => {
    if (!device) return;
    const n = alreadyOnKobo.length;
    try {
      await changeCollections(device.mount, [
        {
          kind: "add",
          name,
          content_ids: alreadyOnKobo.map((b) => onDevice.get(b.id)!.content_id),
        },
      ]);
      setNotice({
        kind: "info",
        text: `Added ${n} ${n === 1 ? "book" : "books"} to “${name}”. Eject your Kobo to see it.`,
      });
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    }
    setDeviceRevision((revision) => revision + 1);
    setCollections(await listCollections(device.mount).catch(() => []));
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
    <div ref={appRef} className={`app ${dragging ? "dragging" : ""}`}>
      <header className="topbar">
        <div className="brand" aria-label="Parallax">
          <img src="/parallax.svg" alt="" />
          <span>Parallax</span>
        </div>
        <div className="tabs" role="tablist">
          <button
            role="tab"
            aria-selected={activeView === "library"}
            title="Library (⌘1)"
            className={activeView === "library" ? "active" : ""}
            onClick={() => setView("library")}
          >
            <LibraryBig />
            Library
          </button>
          {device && (
            <button
              role="tab"
              aria-selected={activeView === "device"}
              title="On Kobo (⌘2)"
              className={activeView === "device" ? "active" : ""}
              onClick={() => {
                setSelected(new Set());
                setView("device");
              }}
            >
              <Tablet />
              On Kobo
            </button>
          )}
          <button
            role="tab"
            aria-selected={activeView === "reading"}
            title="Reading (⌘3)"
            className={activeView === "reading" ? "active" : ""}
            onClick={() => {
              setSelected(new Set());
              setView("reading");
            }}
          >
            <BookOpen />
            Reading
          </button>
        </div>
        <input
          ref={searchRef}
          className="search"
          type="search"
          aria-label="Search"
          placeholder="Search title, author, series, tag  (⌘F)"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <DeviceBadge device={device} onOpen={() => setAboutOpen(true)} />
      </header>

      {/* Retain loaded data, decoded images and scroll positions across tabs.
          Disconnecting unmounts the device panel, so reconnects read fresh data. */}
      {activeView === "reading" && (
        <div className="tab-panel" role="tabpanel" aria-label="Reading">
          <div className="scroll reading-scroll">
            <ReadingView mount={device?.mount ?? null} library={books} />
          </div>
        </div>
      )}
      {device && (
        <div
          className="tab-panel"
          role="tabpanel"
          aria-label="On Kobo"
          hidden={activeView !== "device"}
        >
          <DeviceView
            key={JSON.stringify([device.mount, device.serial])}
            revision={deviceRevision}
            device={device}
            query={query}
            libraryByContentId={libraryByContentId}
            onLibraryChanged={refresh}
            onBooksAdded={(ids) => {
              void refresh().then(() => reviewAdded(ids, [], "kobo"));
            }}
            about={aboutOpen}
            onAboutChange={setAboutOpen}
            onRevert={() => setReverting("kobo")}
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
        </div>
      )}
      <div
        className="tab-panel"
        role="tabpanel"
        aria-label="Library"
        hidden={activeView !== "library"}
      >
        <div className="body">
          <main className="content">
            {books.length > 0 && (
              <section
                className="library-toolbar"
                aria-label="Library controls"
              >
                <div className="view-toolbar">
                  <span className="muted">
                    {visible.length === books.length
                      ? `${books.length} ${books.length === 1 ? "book" : "books"}`
                      : `${visible.length} of ${books.length}`}
                  </span>
                  <ActionMenu
                    label="Library tools"
                    icon={<SlidersHorizontal />}
                    items={[
                      {
                        label: "Find duplicates…",
                        onPick: () => {
                          const byId = new Map(books.map((b) => [b.id, b]));
                          setDuplicates(
                            libraryDuplicates().then((groups) =>
                              libraryReviewGroups(
                                groups,
                                byId,
                                (id) => onDevice.has(id),
                                coverSrc,
                              ),
                            ),
                          );
                        },
                      },
                      {
                        label: "Check library…",
                        onPick: () => setChecking(true),
                      },
                      {
                        label: "Revert changes…",
                        onPick: () => setReverting("library"),
                      },
                      {
                        label: "Import from calibre…",
                        onPick: () => setFromCalibre(true),
                      },
                      { label: "Tidy genres…", onPick: () => setTidying(true) },
                      {
                        label: selected.size
                          ? `Find series & genres for ${selected.size}…`
                          : "Find series & genres…",
                        onPick: () =>
                          setEnhancing(
                            selected.size
                              ? [...selected]
                              : visible.map((b) => b.id),
                          ),
                      },
                    ]}
                  />
                  <span className="spacer" />
                  {selected.size > 0 && (
                    <span className="muted">{selected.size} selected</span>
                  )}
                  <ViewControls
                    mode={mode}
                    onMode={setMode}
                    sort={sort}
                    onSort={setSort}
                    columns={sortColumns}
                    note={colorsLoading ? "Reading cover colours…" : null}
                  />
                </div>
                <LibraryFilters
                  filter={shownFilter}
                  facets={facets}
                  connected={device !== null}
                  onChange={changeFilter}
                />
              </section>
            )}
            <div
              className="scroll"
              onClick={(e) =>
                e.target === e.currentTarget && setSelected(new Set())
              }
            >
              {loaded && books.length === 0 ? (
                <div className="dropzone">
                  <ShelfArt />
                  <p className="dropzone-title">Your shelves are empty</p>
                  <p className="muted">
                    Drop EPUB or DRM-free Kindle books here. They're added to
                    your library, ready to edit and send to your Kobo.
                  </p>
                  <div className="dropzone-actions">
                    <button className="secondary" onClick={chooseFiles}>
                      Choose files…
                    </button>
                    <button
                      className="secondary"
                      onClick={() => setFromCalibre(true)}
                    >
                      Import from calibre…
                    </button>
                  </div>
                </div>
              ) : visible.length === 0 && loaded ? (
                <div className="empty">
                  {isFiltered(shownFilter) ? (
                    <>
                      <p className="dropzone-title">Nothing on this shelf</p>
                      <p className="muted">
                        No books match these filters
                        {query.trim() && ` and “${query}”`}.
                      </p>
                      <button
                        className="secondary"
                        onClick={() => changeFilter(NO_FILTER)}
                      >
                        Clear filters
                      </button>
                    </>
                  ) : (
                    <>
                      <p className="dropzone-title">
                        Nothing on the shelf by that name
                      </p>
                      <p className="muted">No books match “{query}”.</p>
                    </>
                  )}
                </div>
              ) : mode === "grid" ? (
                <div className="grid">
                  {visible.map((b) => (
                    <BookCard
                      key={b.id}
                      book={b}
                      selected={selected.has(b.id)}
                      deviceState={deviceState(b)}
                      status={statuses[b.id]}
                      onSelect={onSelect}
                    />
                  ))}
                </div>
              ) : (
                <div
                  className="book-list"
                  role="table"
                  style={
                    { "--tracks": listTracks(columns) } as React.CSSProperties
                  }
                >
                  <SortHeader columns={columns} sort={sort} onSort={setSort} />
                  {visible.map((b) => (
                    <LibraryRow
                      key={b.id}
                      book={b}
                      selected={selected.has(b.id)}
                      deviceState={deviceState(b)}
                      status={statuses[b.id]}
                      onSelect={onSelect}
                    />
                  ))}
                </div>
              )}
            </div>
          </main>
          <div className="panel-slot">
            {panel ? (
              <EditPanel
                key={`${panel.book.id}-${enhanced}`}
                book={panel.book}
                cover={panel.cover}
                canUpscale={canUpscale}
                onSave={save}
                onEnhance={(id) => setEnhancing([id])}
                onChanged={(updated) => {
                  setBooks((bs) =>
                    bs.map((b) => (b.id === updated.id ? updated : b)),
                  );
                  refreshOnDevice(device);
                }}
                onClose={() => setSelected(new Set())}
              />
            ) : (
              <aside className="panel panel-empty" aria-label="Book details">
                <BookOpen aria-hidden="true" />
                <h2>
                  {selectedBooks.length > 1
                    ? `${selectedBooks.length} books selected`
                    : selectedBooks.length === 1
                      ? "Loading book details…"
                      : "Select a book"}
                </h2>
                <p className="muted">
                  {selectedBooks.length > 1
                    ? "Select one book to view and edit its details."
                    : "View and edit its details here."}
                </p>
              </aside>
            )}
          </div>
        </div>

        <footer className="actions">
          <button
            className="secondary"
            onClick={chooseFiles}
            disabled={busy !== null}
          >
            <Plus />
            {busy === "importing" ? "Adding…" : "Add books…"}
          </button>
          {reviewOffer ? (
            <span className="notice notice-info review-offer">
              {plural(reviewOffer.ids.length, "book")} added ·{" "}
              {flaggedCount(reviewOffer)} could use attention
              <button
                className="link"
                onClick={() => {
                  setReviews((queue) => [...queue, reviewOffer]);
                  setReviewOffer(null);
                }}
              >
                Review
              </button>
              <button className="link" onClick={() => setReviewOffer(null)}>
                Not now
              </button>
            </span>
          ) : notice ? (
            <span className={`notice notice-${notice.kind}`}>
              {notice.text}
            </span>
          ) : (
            <span className="notice notice-info">{SELECTION_HINT}</span>
          )}
          <span className="spacer" />
          {selected.size > 0 && (
            <button
              className="secondary danger"
              onClick={remove}
              disabled={busy !== null}
            >
              <Trash2 />
              Remove
            </button>
          )}
          {device && sentThisSession && (
            <button
              className="secondary"
              onClick={eject}
              disabled={busy !== null}
            >
              <Eject />
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
              <RefreshCw />
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
          {device &&
            device.read_only === null &&
            toSend.length === 0 &&
            alreadyOnKobo.length > 0 && (
              <CollectionMenu
                label={`Add ${alreadyOnKobo.length} to collection`}
                collections={collections}
                disabled={busy !== null}
                onPick={addToCollection}
                onNew={() => setNaming(true)}
              />
            )}
          <button
            className="primary"
            onClick={() =>
              transfer(
                "sending",
                toSend.map((b) => b.id),
              )
            }
            disabled={
              !device ||
              device.read_only !== null ||
              toSend.length === 0 ||
              busy !== null
            }
            title={
              !device
                ? "Connect your Kobo to send books"
                : (device.read_only ?? undefined)
            }
          >
            <Send />
            {busy === "sending"
              ? "Sending…"
              : toSend.length > 0
                ? `Send ${toSend.length} to Kobo`
                : "Send to Kobo"}
          </button>
        </footer>
      </div>
      {naming && (
        <NameDialog
          title={`New collection with ${alreadyOnKobo.length} ${alreadyOnKobo.length === 1 ? "book" : "books"}`}
          confirmLabel="Create"
          onClose={() => setNaming(false)}
          onSubmit={addToCollection}
        />
      )}
      {tidying && (
        <TidyGenres
          mount={device && device.read_only === null ? device.mount : null}
          onDone={() => {
            refresh();
            if (device) {
              setDeviceRevision((revision) => revision + 1);
              listCollections(device.mount)
                .then(setCollections)
                .catch(() => {});
            }
          }}
          onClose={() => setTidying(false)}
        />
      )}
      {fromCalibre && (
        <CalibreImport
          onImported={(ids) => {
            void refresh().then(() => reviewAdded(ids, [], "calibre"));
          }}
          onClose={() => setFromCalibre(false)}
        />
      )}
      {reverting && (
        <RevertChanges
          device={device}
          start={reverting}
          onReverted={(scope, text) => {
            setReverting(null);
            setSelected(new Set());
            setNotice({ kind: "info", text });
            if (scope === "library") void refresh();
            if (device) {
              refreshOnDevice(device);
              listCollections(device.mount)
                .then(setCollections)
                .catch(() => {});
            }
          }}
          onClose={() => setReverting(null)}
        />
      )}
      {checking && (
        <LibraryCheck
          books={new Map(books.map((b) => [b.id, b]))}
          onShow={(id) => {
            setChecking(false);
            setView("library");
            setSelected(new Set([id]));
          }}
          onUpscale={(ids) => {
            setChecking(false);
            setUpscaling(ids);
          }}
          onClose={() => setChecking(false)}
        />
      )}
      {upscaling && (
        <UpscaleReview
          books={books.filter((b) => upscaling.includes(b.id))}
          onChanged={(updated) => {
            setBooks((bs) =>
              bs.map((b) => (b.id === updated.id ? updated : b)),
            );
            refreshOnDevice(device);
          }}
          onClose={() => setUpscaling(null)}
        />
      )}
      {enhancing && (
        <EnhanceReview
          ids={enhancing}
          mount={device?.mount ?? null}
          onApplied={(updated) => {
            const byId = new Map(updated.map((b) => [b.id, b]));
            setBooks((bs) => bs.map((b) => byId.get(b.id) ?? b));
            setEnhanced((n) => n + 1);
          }}
          kobo={
            device
              ? {
                  isOnKobo: (id) => onDevice.has(id),
                  update: async (ids) => {
                    const outcomes = await updateBooks(device.mount, ids);
                    await refreshOnDevice(device);
                    const failed = outcomes.find((o) => "Err" in o.result);
                    if (failed && "Err" in failed.result)
                      throw new Error(failed.result.Err);
                  },
                }
              : undefined
          }
          onClose={() => setEnhancing(null)}
        />
      )}
      {reviews[0] && (
        <ImportReview
          key={reviews[0].ids.join()}
          batch={reviews[0]}
          books={new Map(books.map((b) => [b.id, b]))}
          mount={device?.mount ?? null}
          canUpscale={canUpscale}
          onKobo={(id) => onDevice.has(id)}
          kobo={writable ? { name: writable.model, send: sendReviewed } : null}
          statuses={statuses}
          onChanged={() => {
            void refresh();
            setEnhanced((n) => n + 1);
            refreshOnDevice(device);
          }}
          onClose={() => setReviews((queue) => queue.slice(1))}
        />
      )}
      {duplicates && (
        <DuplicateReview
          place="library"
          groups={duplicates}
          removeWarning="They're removed from your library; copies on your Kobo stay."
          remove={async (ids) => {
            await removeBooks(ids.map(Number));
            setSelected(new Set());
          }}
          dismiss={(key) => dismissDuplicates(key, null)}
          onChanged={refresh}
          onClose={() => setDuplicates(null)}
        />
      )}
    </div>
  );
}
