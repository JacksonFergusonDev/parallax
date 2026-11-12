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
import { ask, message, open } from "@tauri-apps/plugin-dialog";
import {
  booksOnDevice,
  changeCollections,
  coverSrc,
  comicTransferEstimate,
  onSettingChange,
  deviceBooks,
  dismissDuplicates,
  ejectDevice,
  libraryDuplicates,
  getSetting,
  importBooks,
  reviewBooks,
  listBooks,
  goodreadsMatches,
  type GoodreadsMatch,
  listDevices,
  removeBooks,
  removeFromDevice,
  sendBooks,
  updateBook,
  updateBooks,
  type Book,
  type DeviceBook,
  type DeviceInfo,
  type MetadataEdit,
  type Progress,
  type ReportBookProgress,
  type SendOutcome,
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
  type DeviceCopyState,
  type DeviceState,
  type LibraryKey,
  BOOK_EXTENSIONS,
  isBookFile,
  skippedPagesNote,
  summarizeImport,
  seriesGroups,
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
import {
  TransferEstimate,
  type BatchTransferProgress,
} from "./transferEstimate";
import { BatchTransferBar } from "./components/BatchTransferBar";
import { BookCard } from "./components/BookCard";
import { CalibreImport } from "./components/CalibreImport";
import { GoodreadsImport } from "./components/GoodreadsImport";
import { goodreadsSummary } from "./goodreads";
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
import { formatSize, libraryReviewGroups } from "./duplicates";
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
import {
  duringSetup,
  sortCommands,
  useMenuCommands,
  type MenuCommands,
} from "./menu";
import { useBookProgress, type PrepareRemoval } from "./bookProgress";
import { BookProgressBar } from "./components/BookProgressBar";
import { SetupFlow } from "./setup/SetupFlow";
import { KoboNotice } from "./components/KoboNotice";
import { useKoboNotice } from "./koboNotice";
import { UpdateNotice } from "./components/UpdateNotice";
import { useUpdate } from "./updates";
import { useSetup } from "./setup/setup";
import "./styles.css";

const DEVICE_POLL_MS = 2000;
const LIBRARY_KEYS = [
  ...libraryColumns(() => undefined).map((c) => c.key),
  "color" as const,
];
const NEW_COLLECTION = "\u0000new";

type Notice = { kind: "error" | "info"; text: string };
type KoboChange = "updated" | "removed";
const NO_KOBO_CHANGES = new Map<number, KoboChange>();
const NO_DEVICE_COPIES = new Map<number, OnDevice>();

const none = new Set<number>();

export default function App() {
  const [stored, setBooks] = useState<Book[]>([]);
  /** Which books are on your Goodreads shelves, by library id. */
  const [goodreads, setGoodreads] = useState<Record<string, GoodreadsMatch>>(
    {},
  );
  const books = useMemo(
    () =>
      stored.map((b) =>
        goodreads[b.id] ? { ...b, goodreads: goodreads[b.id] } : b,
      ),
    [stored, goodreads],
  );
  const [loaded, setLoaded] = useState(false);
  const appRef = useRef<HTMLDivElement>(null);
  const [detected, setDetected] = useState(false);
  const [view, setView] = useState<View>("library");
  // Opens on the view Settings says, once it's known.
  const themed = usePreferences(setView);
  const setup = useSetup(loaded, books.length);
  usePresentation(appRef, loaded && detected && themed && setup.known);
  const [device, setDevice] = useState<DeviceInfo | null>(null);
  // Setup has its own word about an untested Kobo.
  const koboNotice = useKoboNotice(setup.open ? null : device);
  const update = useUpdate();
  const activeKobo = useRef<DeviceInfo | null>(null);
  const copyRequest = useRef(0);
  const [copies, setCopies] = useState<{
    device: DeviceInfo;
    byId: Map<number, OnDevice>;
  } | null>(null);
  const onDevice = copies?.device === device ? copies.byId : NO_DEVICE_COPIES;
  const [changes, setChanges] = useState<{
    device: DeviceInfo;
    byId: Map<number, KoboChange>;
  } | null>(null);
  const koboChanges =
    changes?.device === device ? changes.byId : NO_KOBO_CHANGES;
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
  const [bySeries, setBySeries] = useState(false);
  const [filter, setFilter] = useState<LibraryFilter>(NO_FILTER);
  const [catalog, setCatalog] = useState<{
    device: DeviceInfo;
    byId: Map<string, DeviceBook>;
  } | null>(null);
  const [aboutOpen, setAboutOpen] = useState(false);
  const { mode, setMode, sort, setSort } = useViewState<LibraryKey>(
    "library",
    LIBRARY_KEYS,
    "grid",
  );
  const searchRef = useRef<HTMLInputElement>(null);
  const [statuses, setStatuses] = useState<Record<number, SendStatus>>({});
  const [batchTransfer, setBatchTransfer] =
    useState<BatchTransferProgress | null>(null);
  const [busy, setBusy] = useState<
    "importing" | "removing" | "sending" | "updating" | "ejecting" | null
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
  const [fromGoodreads, setFromGoodreads] = useState(false);
  const [tidying, setTidying] = useState(false);
  /** Bumped when an enhance saves, so an open edit form reloads. */
  const [enhanced, setEnhanced] = useState(0);
  const [notice, setNotice] = useState<Notice | null>(null);
  /** Added books waiting for review; the first one is open. */
  const [reviews, setReviews] = useState<ReviewBatch[]>([]);
  /** Too many books to open a review for: it's offered instead. */
  const [reviewOffer, setReviewOffer] = useState<ReviewBatch | null>(null);
  const [bookOperation, runBookOperation] = useBookProgress();
  const anchor = useRef<number | null>(null);

  const refresh = useCallback(async () => {
    try {
      setBooks(await listBooks());
      setGoodreads(await goodreadsMatches().catch(() => ({})));
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      setLoaded(true);
    }
  }, []);

  useEffect(() => {
    goodreadsMatches()
      .then(setGoodreads)
      .catch(() => undefined);
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
              ? next && { ...next }
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

  const refreshOnDevice = useCallback(
    async (d: DeviceInfo | null, reload = false) => {
      if (!d || activeKobo.current !== d) return;
      const request = ++copyRequest.current;
      const list = await booksOnDevice(d.mount, reload).catch(() => null);
      if (list && activeKobo.current === d && copyRequest.current === request) {
        setCopies({
          device: d,
          byId: new Map(list.map((o) => [o.book_id, o])),
        });
      }
    },
    [],
  );

  useEffect(() => {
    if (!device) return;
    const unlisten = onSettingChange(({ key }) => {
      if (key.startsWith("comic_")) void refreshOnDevice(device, true);
    });
    return () => {
      unlisten.then((stop) => stop());
    };
  }, [device, refreshOnDevice]);

  // When a Kobo connects: learn which library books are on it, list its
  // collections, and apply series to books it has imported since they were
  // sent (Nickel ignores series in sideloaded EPUBs).
  useEffect(() => {
    let alive = true;
    activeKobo.current = device;
    void refreshOnDevice(device);
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
            setNotice({
              kind: "info",
              text: `Parallax ${text} on your Kobo. Eject to see the changes.`,
            });
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
      activeKobo.current = null;
    };
  }, [device, refreshOnDevice]);

  /** The connected Kobo, if books can be sent to it. */
  const writable = device?.read_only === null ? device : null;
  const sendTo = useRef(writable);
  useEffect(() => {
    sendTo.current = writable;
  });

  const confirmComicTransfer = async (
    target: DeviceInfo,
    ids: number[],
    updating = false,
  ) => {
    const comics = books.filter(
      (book) => book.format === "cbz" && ids.includes(book.id),
    );
    if (comics.length === 0) return true;
    const estimate = await comicTransferEstimate(target.mount, ids);
    if (!estimate) return true;
    const screen =
      estimate.source === "assumed"
        ? " The screen size is assumed."
        : estimate.source === "covers"
          ? " The screen size was estimated from Kobo covers."
          : "";
    const storage =
      estimate.bytes > estimate.free
        ? " The transfer may exceed the available space."
        : "";
    const series =
      !updating && comics.some((book) => book.series)
        ? " Reconnect after the Kobo imports the comics to apply series."
        : "";
    return ask(
      `Prepare comics for ${estimate.width} × ${estimate.height}.${screen}

Estimated transfer: ${formatSize(estimate.bytes)}. Available: ${formatSize(estimate.free)}.${storage}

Notes and bookmarks are unavailable on the Kobo for sideloaded fixed-layout books.${series}`,
      {
        title: updating ? "Update comics on Kobo?" : "Send comics to Kobo?",
        okLabel: updating ? "Update" : "Send",
        cancelLabel: "Cancel",
      },
    );
  };

  /** Comics on the Kobo as their original CBZ only take edited details.
   * Says what updating them will and won't do before it happens. */
  const confirmOriginalComics = (ids: number[]) => {
    const originals = books.filter(
      (b) => ids.includes(b.id) && onDevice.get(b.id)?.original_comic,
    );
    if (originals.length === 0) return Promise.resolve(true);
    const one = originals.length === 1;
    const [it, its, files] = one
      ? ["it", "its", "file"]
      : ["them", "their", "files"];
    const which = one
      ? `“${originals[0].title}” is`
      : `${originals.length} comics are`;
    return ask(
      `${which} on your Kobo as the original CBZ ${files}, so Parallax updates ${its} title, author and series there. You keep your place.

The pages and cover on your Kobo stay as they are, so comic page settings and cover changes won't show. To get a copy prepared for your Kobo's screen instead, remove ${it} from your Kobo and send ${it} again. You'll start from the beginning.`,
      {
        title: "Update comic details?",
        okLabel: "Update Details",
        cancelLabel: "Cancel",
      },
    );
  };

  const rememberKoboChange = (
    target: DeviceInfo,
    ids: number[],
    change: KoboChange,
  ) => {
    if (!ids.length || activeKobo.current !== target) return;
    setChanges((previous) => {
      const byId = new Map(previous?.device === target ? previous.byId : []);
      for (const id of ids) byId.set(id, change);
      return { device: target, byId };
    });
  };

  const updateOnKobo = async (
    target: DeviceInfo,
    ids: number[],
    report?: (p: Progress) => void,
  ) => {
    const outcomes = await updateBooks(target.mount, ids, report);
    rememberKoboChange(
      target,
      outcomes.flatMap(({ book_id, result }) =>
        book_id !== null && "Ok" in result ? [book_id] : [],
      ),
      "updated",
    );
    return outcomes;
  };

  const removeOnKobo = async (
    target: DeviceInfo,
    contentIds: string[],
    report?: ReportBookProgress,
  ) => {
    try {
      const outcomes = await removeFromDevice(
        target.mount,
        contentIds,
        report,
        target.serial,
      );
      const succeeded = outcomes.filter(({ result }) => "Ok" in result);
      const removed = succeeded.flatMap(({ book_id }) =>
        book_id == null ? [] : [book_id],
      );
      rememberKoboChange(target, removed, "removed");
      if (succeeded.length && activeKobo.current === target) {
        setNotice({
          kind: "info",
          text: `Removed ${succeeded.length} ${succeeded.length === 1 ? "book" : "books"} from Kobo. Eject to let your Kobo refresh its library.`,
        });
      }
      if (removed.length && activeKobo.current === target) {
        setCopies((previous) => {
          if (previous?.device !== target) return previous;
          const byId = new Map(previous.byId);
          for (const id of removed) byId.delete(id);
          return { device: target, byId };
        });
        setStatuses((previous) => {
          const next = { ...previous };
          for (const id of removed) delete next[id];
          return next;
        });
      }
      return outcomes;
    } finally {
      await refreshOnDevice(target);
      if (activeKobo.current === target)
        setDeviceRevision((revision) => revision + 1);
    }
  };

  const runTransfer = async (
    ids: number[],
    action: (report: (p: Progress) => void) => Promise<SendOutcome[]>,
  ) => {
    const estimate = new TransferEstimate(ids.length);
    setBatchTransfer(estimate.initial());
    setStatuses(Object.fromEntries(ids.map((id) => [id, { stage: "queued" }])));
    let active = true;
    try {
      const outcomes = await action((p) => {
        if (active) {
          setBatchTransfer(estimate.record(p, performance.now()));
          setStatuses((current) => applyProgress(current, ids, p));
        }
      });
      setStatuses((current) => {
        const next = { ...current };
        for (const { book_id, result } of outcomes) {
          if (book_id !== null)
            next[book_id] =
              "Ok" in result
                ? { stage: "sent" }
                : { stage: "failed", error: result.Err };
        }
        return next;
      });
      return outcomes;
    } catch (error) {
      setStatuses((current) => {
        const next = { ...current };
        for (const id of ids) {
          if (next[id]?.stage !== "sent" && next[id]?.stage !== "failed")
            next[id] = { stage: "failed", error: String(error) };
        }
        return next;
      });
      throw error;
    } finally {
      active = false;
      setBatchTransfer(null);
    }
  };

  /** Sends books from the review, showing progress on them as usual. */
  const sendReviewed = async (ids: number[]) => {
    if (!writable) return [];
    setBusy("sending");
    try {
      if (!(await confirmComicTransfer(writable, ids))) return null;
      return await runTransfer(ids, (report) =>
        sendBooks(writable.mount, ids, null, report),
      );
    } finally {
      await refreshOnDevice(writable);
      if (activeKobo.current === writable)
        setDeviceRevision((revision) => revision + 1);
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
    async (paths: string[], comicFolder = false) => {
      const files = comicFolder ? paths : paths.filter(isBookFile);
      if (files.length === 0) {
        setNotice({
          kind: "error",
          text: "Choose an EPUB, Kindle book, comic, or ZIP, RAR or 7z archive.",
        });
        return;
      }
      setNotice(null);
      try {
        const results = await runBookOperation("Adding", (report) => {
          setBusy("importing");
          return importBooks(files, comicFolder, report);
        });
        setNotice({ kind: "info", text: summarizeImport(results) });
        await refresh();
        void reviewAdded(addedIds(results), failedFiles(results));
      } catch (e) {
        setNotice({ kind: "error", text: String(e) });
      } finally {
        setBusy(null);
      }
    },
    [refresh, reviewAdded, runBookOperation],
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
    (b: Book): DeviceState => {
      const copy = onDevice.get(b.id);
      const change = koboChanges.get(b.id);
      if (!copy) return change === "removed" ? change : undefined;
      if (
        catalog?.device === device &&
        catalog.byId.get(copy.content_id)?.pending_import
      )
        return "pending_import";
      if (copy.stale) return "stale";
      return change === "updated" ? change : "on";
    },
    [onDevice, catalog, device, koboChanges],
  );
  const copyState = useCallback(
    (b: Book): DeviceCopyState => {
      const copy = onDevice.get(b.id);
      return copy && (copy.stale ? "stale" : "on");
    },
    [onDevice],
  );
  // Read again whenever what's on the Kobo or its collections change.
  useEffect(() => {
    if (!device) return;
    let alive = true;
    deviceBooks(device.mount)
      .then((list) => {
        if (alive)
          setCatalog({ device, byId: new Map(list.map((b) => [b.id, b])) });
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [device, onDevice, collections, deviceRevision]);
  const facts = useMemo<BookFacts>(
    () => ({
      deviceState: copyState,
      collectionsOf: (b) => {
        const contentId = onDevice.get(b.id)?.content_id;
        return (
          (contentId &&
            catalog?.device === device &&
            catalog.byId.get(contentId)?.collections) ||
          []
        );
      },
    }),
    [copyState, onDevice, catalog, device],
  );
  const columns = useMemo(() => libraryColumns(copyState), [copyState]);
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
  const groups = useMemo(
    () => (bySeries ? seriesGroups(visible) : []),
    [bySeries, visible],
  );
  const selectionOrder = useMemo(
    () =>
      (bySeries ? groups.flatMap((group) => group.members) : visible).map(
        (b) => b.id,
      ),
    [bySeries, groups, visible],
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
      setSelected((prev) => clickSelect(prev, id, selectionOrder, anchor, e)),
    [selectionOrder],
  );

  const chooseFiles = useCallback(async () => {
    const picked = await open({
      multiple: true,
      filters: [{ name: "Books and archives", extensions: BOOK_EXTENSIONS }],
    });
    if (picked) doImport(Array.isArray(picked) ? picked : [picked]);
  }, [doImport]);

  const chooseComicFolder = async () => {
    const folder = await open({
      directory: true,
      title: "Choose one comic volume",
    });
    if (typeof folder === "string") void doImport([folder], true);
  };

  useEffect(() => {
    // Setup has the window to itself, so its shortcuts would act unseen.
    if (setup.open) return;
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
  }, [visible, chooseFiles, setup.open]);

  const save = async (id: number, edit: MetadataEdit) => {
    const updated = await updateBook(id, edit);
    setBooks((bs) => bs.map((b) => (b.id === id ? updated : b)));
    await refreshOnDevice(device);
  };

  const transfer = async (kind: "sending" | "updating", ids: number[]) => {
    if (!device || ids.length === 0) return;
    let thenEject = false;
    setBusy(kind);
    setNotice(null);
    try {
      if (kind === "updating" && !(await confirmOriginalComics(ids))) return;
      if (
        !(await confirmComicTransfer(
          device,
          kind === "updating"
            ? ids.filter((id) => !onDevice.get(id)?.original_comic)
            : ids,
          kind === "updating",
        ))
      )
        return;
      const collection =
        sendCollection === NEW_COLLECTION
          ? newCollection.trim() || null
          : sendCollection || null;
      const outcomes = await runTransfer(ids, (report) =>
        kind === "sending"
          ? sendBooks(device.mount, ids, collection, report)
          : updateOnKobo(device, ids, report),
      );
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
              text: [
                kind === "sending"
                  ? "Sent. Eject your Kobo and unplug it to import the new books."
                  : "Updated. Eject your Kobo to see the changes.",
                skippedPagesNote(outcomes),
              ]
                .filter(Boolean)
                .join(" "),
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

  const prepareLibraryRemoval: PrepareRemoval<number> = async (ids) => {
    const target = writable;
    const selectedIds = new Set(ids);
    const copies = target
      ? await booksOnDevice(target.mount).catch(() => [])
      : [];
    const contentIds =
      target && activeKobo.current === target
        ? [
            ...new Set(
              copies
                .filter((copy) => selectedIds.has(copy.book_id))
                .map((copy) => copy.content_id),
            ),
          ]
        : [];
    const count = contentIds.length;
    const question = `Remove ${ids.length} ${ids.length === 1 ? "book" : "books"} from your library?`;
    const options = { title: "Remove from library", kind: "warning" } as const;

    let koboCount = 0;
    if (count) {
      const libraryOnly = "Library Only";
      const both = "Library and Kobo";
      const answer = await message(
        `${question} ${count === 1 ? "One selected book has a copy" : `${count} selected books have copies`} on your Kobo. Removing from Kobo also removes the reading progress and highlights stored there.`,
        {
          ...options,
          buttons: { yes: libraryOnly, no: both, cancel: "Cancel" },
        },
      );
      if (answer === both) koboCount = count;
      else if (answer !== libraryOnly) return null;
    } else if (
      !(await ask(question, {
        ...options,
        okLabel: "Remove",
        cancelLabel: "Cancel",
      }))
    ) {
      return null;
    }
    const total = ids.length + koboCount;
    return {
      starting: koboCount ? "Backing up Kobo…" : "Saving a library snapshot…",
      run: async (report) => {
        try {
          if (target && koboCount) {
            if (activeKobo.current !== target) {
              throw new Error(
                "The Kobo connection changed. Confirm the removal again.",
              );
            }
            const outcomes = await removeOnKobo(
              target,
              contentIds,
              (progress) => report({ ...progress, total }),
            );
            const failure = outcomes.find((outcome) => "Err" in outcome.result);
            if (failure && "Err" in failure.result) {
              throw new Error(
                `Could not remove every selected copy from Kobo. Your library books were kept. ${failure.result.Err}`,
              );
            }
          }
          await removeBooks(ids, (progress) =>
            report({
              ...progress,
              total,
              completed: koboCount + progress.completed,
            }),
          );
          setSelected(new Set());
        } finally {
          await refresh();
        }
      },
    };
  };

  const remove = async () => {
    if (!selected.size || busy) return;
    setBusy("removing");
    try {
      const removal = await prepareLibraryRemoval([...selected]);
      if (!removal) return;
      setNotice(null);
      await runBookOperation("Removing", removal.run, removal.starting);
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
      setChanges(null);
      setStatuses({});
      setNotice({
        kind: "info",
        text: "Ejected. Unplug your Kobo to let it refresh its library.",
      });
    } catch (e) {
      setNotice({ kind: "error", text: String(e) });
    } finally {
      setBusy(null);
    }
  };

  const findDuplicates = () => {
    const byId = new Map(books.map((b) => [b.id, b]));
    setDuplicates(
      libraryDuplicates().then((groups) =>
        libraryReviewGroups(groups, byId, (id) => onDevice.has(id), coverSrc),
      ),
    );
  };
  const enhance = () =>
    setEnhancing(selected.size ? [...selected] : visible.map((b) => b.id));

  const canSend =
    !!device && device.read_only === null && toSend.length > 0 && busy === null;
  const inLibrary = activeView === "library";
  /** Runs `run` in the library view, where its results show. */
  const fromLibrary = (run: () => void) => () => {
    setView("library");
    run();
  };
  const commands: MenuCommands = {
    "help.welcome": { run: setup.show, enabled: !setup.open },
    "file.add": { run: () => void chooseFiles() },
    "file.calibre": { run: () => setFromCalibre(true) },
    "file.goodreads": { run: () => setFromGoodreads(true) },
    "edit.find": {
      run: () => {
        searchRef.current?.focus();
        searchRef.current?.select();
      },
    },
    "view.library": {
      run: () => setView("library"),
      checked: activeView === "library",
    },
    "view.device": {
      run: () => setView("device"),
      enabled: device !== null,
      checked: activeView === "device",
    },
    "view.reading": {
      run: () => setView("reading"),
      checked: activeView === "reading",
    },
    ...(inLibrary && {
      "view.grid": { run: () => setMode("grid"), checked: mode === "grid" },
      "view.list": { run: () => setMode("list"), checked: mode === "list" },
      ...sortCommands(sort, setSort, LIBRARY_KEYS),
    }),
    "library.enhance": {
      run: fromLibrary(enhance),
      enabled: books.length > 0,
    },
    "library.duplicates": {
      run: fromLibrary(findDuplicates),
      enabled: books.length > 1,
    },
    "library.check": {
      run: () => setChecking(true),
      enabled: books.length > 0,
    },
    "library.tidy": { run: () => setTidying(true), enabled: books.length > 0 },
    "library.remove": {
      run: () => void remove(),
      enabled: inLibrary && selected.size > 0 && busy === null,
    },
    "library.revert": { run: () => setReverting("library") },
    "kobo.send": {
      run: () =>
        void transfer(
          "sending",
          toSend.map((b) => b.id),
        ),
      enabled: canSend,
    },
    "kobo.update": {
      run: () =>
        void transfer(
          "updating",
          toUpdate.map((b) => b.id),
        ),
      enabled: !!device && toUpdate.length > 0 && busy === null,
    },
    "kobo.eject": {
      run: () => void eject(),
      enabled: device !== null && busy === null,
    },
  };
  useMenuCommands(setup.open ? duringSetup(commands) : commands);

  const transferStatus = (id: number) => {
    const status = device ? statuses[id] : undefined;
    return status?.stage === "sent" ? undefined : status;
  };
  const renderCard = (b: Book) => (
    <BookCard
      key={b.id}
      book={b}
      selected={selected.has(b.id)}
      deviceState={deviceState(b)}
      status={transferStatus(b.id)}
      onSelect={onSelect}
    />
  );

  const hasChangesToSee =
    koboChanges.size > 0 ||
    (catalog?.device === device &&
      [...catalog.byId.values()].some((b) => b.pending_import));

  return (
    <div ref={appRef} className={`app ${dragging ? "dragging" : ""}`}>
      {/* Setup covers the window; what's behind it keeps its state. */}
      <div className="app-main" inert={setup.open}>
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
        {update.update && !setup.open && (
          <UpdateNotice
            update={update.update}
            stage={update.stage}
            busy={busy !== null}
            onInstall={() => void update.install()}
            onSkip={() => void update.skip().catch(console.error)}
            onClose={update.close}
          />
        )}
        {koboNotice.notice && (
          <KoboNotice
            notice={koboNotice.notice}
            standing={koboNotice.standing}
            onClose={koboNotice.close}
            onDismiss={koboNotice.dismiss}
          />
        )}

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
              koboChanges={koboChanges}
              onRemove={(ids, report) => removeOnKobo(device, ids, report)}
              onUpdate={(ids) => updateOnKobo(device, ids)}
              onLibraryChanged={refresh}
              onBooksAdded={(ids) => {
                void refresh().then(() => reviewAdded(ids, [], "kobo"));
              }}
              about={aboutOpen}
              onAboutChange={setAboutOpen}
              onRevert={() => setReverting("kobo")}
              active={activeView === "device"}
              onShow={() => setView("device")}
              onChanged={() => {
                refreshOnDevice(device);
                listCollections(device.mount)
                  .then(setCollections)
                  .catch(() => {});
              }}
              onEjected={() => {
                setDevice(null);
                setChanges(null);
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
                          label: "Add comic folder…",
                          onPick: () => void chooseComicFolder(),
                          disabled: busy !== null,
                        },
                        {
                          label: "Find duplicates…",
                          onPick: findDuplicates,
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
                        {
                          label: "Import from Goodreads…",
                          onPick: () => setFromGoodreads(true),
                        },
                        {
                          label: "Tidy genres…",
                          onPick: () => setTidying(true),
                        },
                        {
                          label: selected.size
                            ? `Find metadata for ${selected.size}…`
                            : "Find metadata…",
                          onPick: enhance,
                        },
                      ]}
                    />
                    <span className="spacer" />
                    {selected.size > 0 && (
                      <span className="muted">{selected.size} selected</span>
                    )}
                    <button
                      className="group-series"
                      aria-pressed={bySeries}
                      onClick={() => setBySeries((value) => !value)}
                    >
                      Group by series
                    </button>
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
                      Drop EPUBs, DRM-free Kindle books, comics, or archives of
                      books here.
                    </p>
                    <div className="dropzone-actions">
                      <button
                        className="secondary"
                        onClick={() => void chooseComicFolder()}
                        disabled={busy !== null}
                      >
                        Add comic folder…
                      </button>
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
                ) : bySeries && mode === "grid" ? (
                  groups.map(({ name, members }) => (
                    <section className="series-shelf" key={name ?? ""}>
                      <h2>
                        {name ?? "Without a series"}{" "}
                        <span className="muted">· {members.length}</span>
                      </h2>
                      <div className="grid">{members.map(renderCard)}</div>
                    </section>
                  ))
                ) : mode === "grid" ? (
                  <div className="grid">{visible.map(renderCard)}</div>
                ) : (
                  <div
                    className="book-list library-list"
                    role="table"
                    style={
                      { "--tracks": listTracks(columns) } as React.CSSProperties
                    }
                  >
                    <SortHeader
                      columns={columns}
                      sort={sort}
                      onSort={setSort}
                    />
                    {(bySeries
                      ? groups
                      : [{ name: null, members: visible }]
                    ).map(({ name, members }) => (
                      <div role="rowgroup" key={name ?? ""}>
                        {bySeries && (
                          <div role="row" className="series-row">
                            <span role="cell" aria-colspan={columns.length}>
                              {name ?? "Without a series"} · {members.length}
                            </span>
                          </div>
                        )}
                        {members.map((b) => (
                          <LibraryRow
                            key={b.id}
                            book={b}
                            selected={selected.has(b.id)}
                            deviceState={deviceState(b)}
                            status={transferStatus(b.id)}
                            onSelect={onSelect}
                          />
                        ))}
                      </div>
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
                    if (shownFilter.kind && updated.kind !== shownFilter.kind) {
                      setSelected((current) => {
                        const next = new Set(current);
                        next.delete(updated.id);
                        return next;
                      });
                    }
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
            {batchTransfer && !reviews[0] && (
              <BatchTransferBar progress={batchTransfer} />
            )}
            <button
              className="secondary"
              onClick={chooseFiles}
              disabled={busy !== null}
            >
              <Plus />
              {busy === "importing" ? "Adding…" : "Add books…"}
            </button>
            {bookOperation ? (
              <BookProgressBar operation={bookOperation} />
            ) : reviewOffer ? (
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
              <span
                className={`notice notice-${notice.kind}`}
                title={notice.text}
              >
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
            {device && hasChangesToSee && (
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
                title="Send your edits to the copies on your Kobo, keeping reading progress"
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
      </div>
      {setup.open && (
        <SetupFlow
          device={device}
          libraryBooks={books.length}
          onKobo={onDevice.size}
          adding={busy === "importing"}
          onLibraryChanged={refresh}
          onChooseFiles={() => void chooseFiles()}
          onChooseComicFolder={() => void chooseComicFolder()}
          onCalibre={() => setFromCalibre(true)}
          onFinish={(outcome) => {
            setup.finish(outcome);
            setView("library");
          }}
        />
      )}
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
      {fromGoodreads && (
        <GoodreadsImport
          onClose={() => setFromGoodreads(false)}
          onImported={(report) => {
            goodreadsMatches().then(setGoodreads, () => undefined);
            setNotice({ kind: "info", text: goodreadsSummary(report) });
          }}
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
              if (scope === "kobo") {
                setChanges(null);
                setStatuses({});
              }
              setDeviceRevision((revision) => revision + 1);
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
                    const outcomes = await updateOnKobo(device, ids);
                    await refreshOnDevice(device);
                    setDeviceRevision((revision) => revision + 1);
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
          batchTransfer={batchTransfer}
          prepareRemove={prepareLibraryRemoval}
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
          prepareRemove={(ids) => prepareLibraryRemoval(ids.map(Number))}
          dismiss={(key) => dismissDuplicates(key, null)}
          onChanged={refresh}
          onClose={() => setDuplicates(null)}
        />
      )}
    </div>
  );
}
