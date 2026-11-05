import { useEffect, useMemo, useState } from "react";
import {
  coverSrc,
  readingStats,
  type Book,
  type SavedStats,
  type StatBook,
} from "../api";
import { spineColor } from "../coverArt";
import {
  finishedByMonth,
  finishedByYear,
  hours,
  readingHeadline,
  ticks,
  titleKey,
  type Bucket,
} from "../stats";
import { GeneratedCover } from "./GeneratedCover";
import { ShelfArt } from "./ShelfArt";

interface Props {
  /** The connected Kobo; null shows the last saved stats. */
  mount: string | null;
  /** The library, for real covers of books being read. */
  library: Book[];
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
export function ReadingView({ mount, library }: Props) {
  const [saved, setSaved] = useState<SavedStats | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [range, setRange] = useState<"years" | "months">("years");

  useEffect(() => {
    let alive = true;
    readingStats(mount)
      .then((s) => {
        if (!alive) return;
        setSaved(s);
        setError(null);
      })
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [mount]);

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

  if (saved === undefined && !error)
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
      <h2 className="reading-headline">{readingHeadline(stats, now)}</h2>
      <p className="muted reading-source">
        {stats.device_model}
        {mount
          ? " · connected now"
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
        <Tile label="Reading now" value={String(stats.reading.length)} />
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
          <ColumnChart buckets={buckets} />
        ) : (
          <p className="muted">No finished books yet.</p>
        )}
      </section>

      {stats.reading.length > 0 && (
        <section className="reading-section">
          <h3>
            Reading now <span className="muted">{stats.reading.length}</span>
          </h3>
          <ul className="now-reading">
            {stats.reading.map((b) => {
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

function Tile({
  label,
  value,
  note,
}: {
  label: string;
  value: string;
  note?: string;
}) {
  return (
    <div className="stat-tile">
      <span className="stat-label">{label}</span>
      <span className="stat-value">{value}</span>
      {note && <span className="muted stat-note">{note}</span>}
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

/** One series of columns: a hover tooltip per column, the tallest labelled. */
function ColumnChart({ buckets }: { buckets: Bucket[] }) {
  const [hover, setHover] = useState<number | null>(null);
  const max = Math.max(...buckets.map((b) => b.count));
  const axis = ticks(max);
  const top = axis[axis.length - 1];
  const peak = buckets.findIndex((b) => b.count === max && max > 0);
  // Each finished book is drawn as a spine while they stay thick enough.
  const stacked = top <= 30;

  return (
    <div
      className="column-chart"
      role="img"
      aria-label={`Books finished: ${buckets.map((b) => `${b.label} ${b.count}`).join(", ")}`}
    >
      <div className="chart-plot">
        {axis.map((t) => (
          <div
            key={t}
            className="gridline"
            style={{ bottom: `${(t / top) * 100}%` }}
          >
            <span className="tick">{t}</span>
          </div>
        ))}
        <div className="columns">
          {buckets.map((b, i) => (
            <div
              key={b.key}
              className="column-slot"
              onMouseEnter={() => setHover(i)}
              onMouseLeave={() => setHover(null)}
            >
              {i === peak && hover === null && (
                <span
                  className="column-value"
                  style={{ bottom: `${(b.count / top) * 100}%` }}
                >
                  {b.count}
                </span>
              )}
              {b.count > 0 &&
                (stacked ? (
                  <div
                    className={`spine-stack ${hover === i ? "hovered" : ""}`}
                    style={{ height: `${(b.count / top) * 100}%` }}
                  >
                    {Array.from({ length: b.count }, (_, n) => (
                      <span
                        key={n}
                        className="spine"
                        style={
                          {
                            "--spine": spineColor(`${b.key}:${n}`),
                            "--tilt": `${((n * 7) % 5) - 2}px`,
                            "--w": `${84 + ((n * 13) % 17)}%`,
                          } as React.CSSProperties
                        }
                      />
                    ))}
                  </div>
                ) : (
                  <div
                    className={`column ${hover === i ? "hovered" : ""}`}
                    style={{ height: `${(b.count / top) * 100}%` }}
                  />
                ))}
              {hover === i && (
                <div
                  className="chart-tooltip"
                  style={{ bottom: `calc(${(b.count / top) * 100}% + 8px)` }}
                >
                  <strong>{b.count}</strong> {b.count === 1 ? "book" : "books"}
                  <span className="muted"> · {b.key}</span>
                </div>
              )}
            </div>
          ))}
        </div>
      </div>
      <div className="column-labels">
        {buckets.map((b) => (
          <span key={b.key}>{b.label}</span>
        ))}
      </div>
    </div>
  );
}
