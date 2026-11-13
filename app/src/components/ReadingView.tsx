import { useEffect, useMemo, useRef, useState } from "react";
import {
  coverSrc,
  peekSavedReadingStats,
  refreshReadingStats,
  savedReadingStats,
  type Book,
  type SavedStats,
  type StatBook,
} from "../api";
import {
  finishedByMonth,
  finishedByYear,
  hours,
  readingHeadline,
  startedBooks,
  titleKey,
} from "../stats";
import { ColumnChart, Tile } from "./charts";
import { GeneratedCover } from "./GeneratedCover";
import { ShelfArt } from "./ShelfArt";

interface Props {
  /** The connected Kobo; null shows the last saved stats. */
  mount: string | null;
  /** The library, for real covers of books being read. */
  library: Book[];
  /** On screen. A connected Kobo is only read while it is. */
  active: boolean;
}

const dateFmt = (iso: string | null) =>
  iso
    ? new Date(iso).toLocaleDateString(undefined, {
        day: "numeric",
        month: "short",
        year: "numeric",
      })
    : "";

/** Reading statistics from the Kobo, or the last ones saved. */
export function ReadingView({ mount, library, active }: Props) {
  // Saved stats prefetched at launch draw on the first render, with no
  // loading state; a connected Kobo's fresh ones replace them when read.
  const [saved, setSaved] = useState<SavedStats | null | undefined>(
    peekSavedReadingStats,
  );
  const [error, setError] = useState<string | null>(null);
  const [range, setRange] = useState<"years" | "months">("years");
  /** The Kobo whose stats are on screen, read this session. */
  const [fresh, setFresh] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const refreshed = useRef<string | null>(null);

  useEffect(() => {
    if (saved !== undefined) return;
    let alive = true;
    savedReadingStats()
      .then((s) => alive && setSaved((current) => current ?? s))
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [saved]);

  // A read finishing after its Kobo left, or after the view did, is dropped.
  const current = useRef(mount);
  const gone = useRef(false);
  useEffect(() => {
    current.current = mount;
    if (!mount) refreshed.current = null;
  }, [mount]);
  useEffect(() => {
    // Strict Mode mounts twice; the second mount is live again.
    gone.current = false;
    return () => {
      gone.current = true;
    };
  }, []);

  // Read a connected Kobo once per connection, and only when its stats are
  // looked at: it reads the device over USB. Looking away mid-read doesn't
  // cancel it.
  useEffect(() => {
    if (!mount || !active || refreshed.current === mount) return;
    refreshed.current = mount;
    const still = () => !gone.current && current.current === mount;
    setRefreshing(true);
    refreshReadingStats(mount)
      .then((s) => {
        if (!still()) return;
        setSaved(s);
        setFresh(mount);
        setError(null);
      })
      .catch((e) => still() && setError(String(e)))
      .finally(() => !gone.current && setRefreshing(false));
  }, [mount, active]);

  const now = useMemo(() => new Date(), []);
  const covers = useMemo(() => {
    const m = new Map<string, string>();
    for (const b of library) {
      const src = coverSrc(b);
      if (src) m.set(titleKey(b.title), src);
    }
    return m;
  }, [library]);
  const stats = saved?.stats;
  const buckets = useMemo(
    () =>
      stats
        ? range === "years"
          ? finishedByYear(stats.finished, now)
          : finishedByMonth(stats.finished, now)
        : [],
    [stats, range, now],
  );

  if ((saved === undefined || (!saved && refreshing)) && !error)
    return <p className="muted empty">Reading your stats…</p>;
  if (!stats)
    return (
      <div className="reading-empty">
        {error && <p className="notice notice-error wrap">{error}</p>}
        <ShelfArt />
        <p className="dropzone-title">No reading stats yet</p>
        <p className="muted">
          Connect your Kobo once and its reading time, books in progress and
          finished books show here, even after you unplug it.
        </p>
      </div>
    );

  const reading = startedBooks(stats.reading);
  const thisYear = stats.finished.filter(
    (b) =>
      b.finished_at &&
      new Date(b.finished_at).getFullYear() === now.getFullYear(),
  ).length;
  const byYear = new Map<string, StatBook[]>();
  for (const b of stats.finished) {
    const y = b.finished_at
      ? String(new Date(b.finished_at).getFullYear())
      : "Undated";
    byYear.set(y, [...(byYear.get(y) ?? []), b]);
  }

  return (
    <div className="reading-page">
      <h2 className="reading-headline">
        {readingHeadline({ ...stats, reading }, now)}
      </h2>
      <p className="muted reading-source">
        {stats.device_model}
        {mount && fresh === mount
          ? " · connected now"
          : refreshing
            ? ` · as of ${dateFmt(saved!.taken_at)}, updating from your Kobo…`
            : ` · as of ${dateFmt(saved!.taken_at)}, when it was last connected`}
        {error && ` · couldn't refresh: ${error}`}
      </p>

      <div className="stat-tiles">
        <Tile
          label="Time read"
          value={hours(stats.total_secs)}
          note={`across ${stats.books_on_device} books on the Kobo`}
        />
        <Tile
          label="Books finished"
          value={String(stats.finished.length)}
          note="since you started"
        />
        <Tile
          label={`Finished in ${now.getFullYear()}`}
          value={String(thisYear)}
        />
        <Tile label="Reading now" value={String(reading.length)} />
      </div>

      <section className="reading-section">
        <div className="section-head">
          <h3>Books finished</h3>
          <div className="segmented" role="group" aria-label="Range">
            <button
              aria-pressed={range === "years"}
              className={range === "years" ? "active" : ""}
              onClick={() => setRange("years")}
            >
              By year
            </button>
            <button
              aria-pressed={range === "months"}
              className={range === "months" ? "active" : ""}
              onClick={() => setRange("months")}
            >
              Last 12 months
            </button>
          </div>
        </div>
        {buckets.length ? (
          <ColumnChart
            buckets={buckets}
            unit={["book", "books"]}
            label="Books finished"
          />
        ) : (
          <p className="muted">No finished books yet.</p>
        )}
      </section>

      {reading.length > 0 && (
        <section className="reading-section">
          <h3>
            Reading now <span className="muted">{reading.length}</span>
          </h3>
          <ul className="now-reading">
            {reading.map((b) => {
              const src = covers.get(titleKey(b.title));
              return (
                <li key={b.content_id} className="now-card">
                  <div className="now-cover">
                    {src ? (
                      <img src={src} alt="" draggable={false} />
                    ) : (
                      <GeneratedCover title={b.title} size="mini" />
                    )}
                  </div>
                  <div className="now-body">
                    <BookName book={b} author={false} />
                    {b.author && (
                      <span className="muted now-author">{b.author}</span>
                    )}
                    <div className="progress" title={`${b.percent}% read`}>
                      <div
                        className="progress-fill"
                        style={{ width: `${b.percent}%` }}
                      />
                    </div>
                    <div className="now-meta">
                      <span className="now-percent">{b.percent}%</span>
                      <span className="muted" title="Time spent reading">
                        {hours(b.time_secs)} read
                      </span>
                      {b.left_secs ? (
                        <span
                          className="muted"
                          title="Kobo's estimate of the time left"
                        >
                          · {hours(b.left_secs)} left
                        </span>
                      ) : null}
                    </div>
                  </div>
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {stats.most_time.length > 0 && (
        <section className="reading-section">
          <h3>Most time spent</h3>
          <ul className="stat-list">
            {stats.most_time.map((b) => (
              <li key={b.content_id} className="stat-row compact">
                <BookName book={b} />
                <span className="stat-num">{hours(b.time_secs)}</span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {stats.finished.length > 0 && (
        <section className="reading-section">
          <h3>
            Finished <span className="muted">{stats.finished.length}</span>
          </h3>
          <p className="muted reading-note">
            From the Kobo's own history, including books removed since. Titles
            of removed books come from their file names.
          </p>
          {[...byYear].map(([year, list]) => (
            <div key={year} className="finished-year">
              <h4>
                {year} <span className="muted">{list.length}</span>
              </h4>
              <ul className="stat-list">
                {list.map((b) => (
                  <li key={b.content_id} className="stat-row compact">
                    <BookName book={b} />
                    <span className="muted stat-num">
                      {b.times_finished > 1 && `${b.times_finished}× · `}
                      {dateFmt(b.finished_at)}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </section>
      )}
    </div>
  );
}

function BookName({
  book,
  author = true,
}: {
  book: StatBook;
  author?: boolean;
}) {
  return (
    <span
      className="stat-book"
      title={book.author ? `${book.title} by ${book.author}` : book.title}
    >
      <span className="stat-title">{book.title}</span>
      {author && book.author && <span className="muted"> · {book.author}</span>}
      {!book.on_device && <span className="muted"> · removed</span>}
    </span>
  );
}
