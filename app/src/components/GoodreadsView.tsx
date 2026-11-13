import { useEffect, useMemo, useState } from "react";
import {
  goodreadsInsights,
  peekGoodreadsInsights,
  type GoodreadsInsights,
} from "../api";
import {
  dateFmt,
  decadeBuckets,
  importLine,
  insightsHeadline,
  lengthBars,
  ratingBars,
  spokenDays,
  yearBuckets,
  type InsightBook,
  type TableFilter,
} from "../goodreadsInsights";
import { BarList, ColumnChart, Tile, num } from "./charts";
import { GoodreadsTable } from "./GoodreadsTable";
import { ShelfArt } from "./ShelfArt";

interface Props {
  /** Changes whenever the saved Goodreads books may have, to read them again. */
  stamp: unknown;
  /** Opens the Goodreads import. */
  onImport: () => void;
}

/** To-read books in your library shown by name; the table has the rest. */
const READY_CHIPS = 12;

/** What your saved Goodreads shelves say about your reading. */
export function GoodreadsView({ stamp, onImport }: Props) {
  // Insights prefetched at launch draw on the first render; the read below
  // then only refreshes them after an import or edit.
  const [insights, setInsights] = useState<GoodreadsInsights | null>(
    () => peekGoodreadsInsights() ?? null,
  );
  const [error, setError] = useState<string | null>(null);
  const [metric, setMetric] = useState<"books" | "pages">("books");
  const [filter, setFilter] = useState<TableFilter>({
    status: "all",
    inLibrary: false,
  });

  useEffect(() => {
    let alive = true;
    goodreadsInsights()
      .then((i) => {
        if (!alive) return;
        setInsights(i);
        setError(null);
      })
      .catch((e) => alive && setError(String(e)));
    return () => {
      alive = false;
    };
  }, [stamp]);

  const byId = useMemo(
    () => new Map(insights?.books.map((b) => [b.goodreads_id, b]) ?? []),
    [insights],
  );

  if (!insights && !error)
    return <p className="muted empty">Reading your Goodreads books…</p>;
  if (!insights || insights.totals.books === 0)
    return (
      <div className="reading-empty">
        {error && <p className="notice notice-error wrap">{error}</p>}
        <ShelfArt />
        <p className="dropzone-title">No Goodreads books yet</p>
        <p className="muted">
          Import your Goodreads library export and you can see how much you read
          each year, how you rate books, your to-read pile and every book on
          your shelves.
        </p>
        <button className="primary" onClick={onImport}>
          Import from Goodreads…
        </button>
      </div>
    );

  const { totals: t, ratings, to_read: pile, wait, lengths, pace } = insights;
  const title = (id: string | null) => (id ? byId.get(id)?.title : undefined);
  const ready = insights.books.filter(
    (b) => b.shelf === "to_read" && b.library.length > 0,
  );
  const longest = lengths.longest ? byId.get(lengths.longest) : undefined;
  const shortest = lengths.shortest ? byId.get(lengths.shortest) : undefined;
  const oldest = pile.oldest ? byId.get(pile.oldest) : undefined;
  const undated = t.read_undated;

  return (
    <div className="reading-page">
      <h2 className="reading-headline">{insightsHeadline(insights)}</h2>
      <p className="muted reading-source">
        <span
          title={[
            insights.import?.file_name,
            insights.import?.latest_activity &&
              `Latest change in it: ${dateFmt(insights.import.latest_activity)}`,
          ]
            .filter(Boolean)
            .join("\n")}
        >
          {importLine(insights.import, insights.imports)}
        </span>
        {" · "}
        <button className="link" onClick={onImport}>
          Import a newer export
        </button>
        {error && ` · couldn't refresh: ${error}`}
      </p>

      <div className="stat-tiles">
        <Tile
          label="Books read"
          value={num(t.read)}
          note={`${num(t.pages_read)} pages`}
        />
        <Tile
          label="To read"
          value={num(t.to_read)}
          note={
            pile.in_library > 0
              ? `${num(pile.in_library)} already in your library`
              : `${num(pile.pages)} pages`
          }
        />
        <Tile
          label="Your average rating"
          value={ratings.average === null ? "—" : ratings.average.toFixed(1)}
          note={`over ${num(t.rated)} rated ${t.rated === 1 ? "book" : "books"}`}
        />
        {pace && (
          <Tile
            label={`Read in ${pace.year}`}
            value={num(pace.this_year)}
            note={`${num(pace.last_year_by_now)} by this time last year`}
          />
        )}
      </div>

      {insights.years.length > 0 && (
        <section className="reading-section">
          <div className="section-head">
            <h3>Read each year</h3>
            <div className="segmented" role="group" aria-label="Measure">
              {(["books", "pages"] as const).map((m) => (
                <button
                  key={m}
                  aria-pressed={metric === m}
                  className={metric === m ? "active" : ""}
                  onClick={() => setMetric(m)}
                >
                  {m === "books" ? "Books" : "Pages"}
                </button>
              ))}
            </div>
          </div>
          <ColumnChart
            buckets={yearBuckets(insights.years, metric)}
            unit={metric === "books" ? ["book", "books"] : ["page", "pages"]}
            label={
              metric === "books"
                ? "Books read each year"
                : "Pages read each year"
            }
            spines={metric === "books"}
          />
          {undated > 0 && (
            <p className="muted reading-note">
              {undated === t.read
                ? "None of your read books has a date read, so they can't be placed in a year."
                : `${num(undated)} of your ${num(t.read)} read books have no date read on Goodreads, so they aren't counted here.`}
            </p>
          )}
        </section>
      )}

      <div className="insight-grid">
        <section className="reading-section">
          <h3>How you rate</h3>
          <BarList
            bars={ratingBars(ratings.stars)}
            label="Books by your rating"
          />
          <p className="muted reading-note">
            {ratings.unrated > 0 &&
              `${num(ratings.unrated)} read ${ratings.unrated === 1 ? "book has" : "books have"} no rating. `}
            {ratings.against_goodreads && ratingGap(ratings.against_goodreads)}
          </p>
        </section>
        <section className="reading-section">
          <h3>Book lengths</h3>
          <BarList
            bars={lengthBars(lengths.buckets, "read")}
            label="Read books by length"
          />
          {lengths.average !== null && (
            <p className="muted reading-note">
              Your read books average {num(lengths.average)} pages.
              {longest &&
                ` The longest is ${longest.title} (${num(longest.pages ?? 0)})`}
              {shortest &&
                shortest !== longest &&
                `, the shortest ${shortest.title} (${num(shortest.pages ?? 0)})`}
              {longest && "."}
            </p>
          )}
        </section>
      </div>

      {(pile.books > 0 || wait) && (
        <section className="reading-section">
          <h3>
            Your to-read pile <span className="muted">{num(pile.books)}</span>
          </h3>
          <ul className="insight-facts">
            {pile.books > 0 && (
              <li>
                {num(pile.books)} books and {num(pile.pages)} pages are waiting.
                {pile.median_age_days !== null &&
                  ` Half of them have waited over ${spokenDays(pile.median_age_days)}`}
                {oldest?.date_added &&
                  `, and ${oldest.title} has been there since ${dateFmt(oldest.date_added)}`}
                {pile.median_age_days !== null && "."}
              </li>
            )}
            {pile.years_to_clear !== null && (
              <li>
                At your pace over the last 12 months ({num(pile.read_last_year)}{" "}
                dated {pile.read_last_year === 1 ? "read" : "reads"}), the pile
                would take {pile.years_to_clear.toFixed(1)} years to finish.
              </li>
            )}
            {wait && (
              <li>
                Books you read sat on your shelves for{" "}
                {spokenDays(wait.median_days)} at the median before you read
                them. {title(wait.longest)} waited longest,{" "}
                {spokenDays(wait.longest_days)}.
              </li>
            )}
          </ul>
          {ready.length > 0 && (
            <>
              <h4>
                Ready in your library{" "}
                <span className="muted">{num(ready.length)}</span>
              </h4>
              <ul className="insight-chips">
                {ready.slice(0, READY_CHIPS).map((b) => (
                  <li key={b.goodreads_id} title={b.authors[0]}>
                    {b.title}
                  </li>
                ))}
              </ul>
              <button
                className="link insight-more"
                onClick={() => {
                  setFilter({ status: "to_read", inLibrary: true });
                  document.getElementById("goodreads-books")?.scrollIntoView({
                    behavior: matchMedia("(prefers-reduced-motion: reduce)")
                      .matches
                      ? "auto"
                      : "smooth",
                    block: "start",
                  });
                }}
              >
                {ready.length > READY_CHIPS
                  ? `See all ${num(ready.length)} in the table`
                  : "See them in the table"}
              </button>
            </>
          )}
        </section>
      )}

      {(insights.authors.length > 0 || insights.series.length > 0) && (
        <div className="insight-grid">
          {insights.authors.length > 0 && (
            <section className="reading-section">
              <h3>Authors you read most</h3>
              <BarList
                label="Books read by author"
                bars={insights.authors.map((a) => ({
                  key: a.name,
                  label: a.name,
                  value: a.read,
                  note:
                    a.average === null
                      ? undefined
                      : `${a.average.toFixed(1)} ★`,
                }))}
              />
            </section>
          )}
          {insights.series.length > 0 && (
            <section className="reading-section">
              <h3>
                Series in progress{" "}
                <span className="muted">{insights.series.length}</span>
              </h3>
              <ul className="stat-list series-list">
                {insights.series.slice(0, 12).map((s) => (
                  <li key={s.name} className="series-item">
                    <span className="stat-book" title={s.author ?? undefined}>
                      <span className="stat-title">{s.name}</span>
                      {s.author && <span className="muted"> · {s.author}</span>}
                    </span>
                    <span className="muted">{seriesNote(s)}</span>
                  </li>
                ))}
              </ul>
              {insights.series.length > 12 && (
                <p className="muted reading-note">
                  And {insights.series.length - 12} more.
                </p>
              )}
            </section>
          )}
        </div>
      )}

      <div className="insight-grid">
        {insights.decades.length > 0 && (
          <section className="reading-section">
            <h3>When they were first published</h3>
            <ColumnChart
              buckets={decadeBuckets(insights.decades)}
              unit={["book", "books"]}
              label="Books by decade first published"
              spines={false}
            />
          </section>
        )}
        <section className="reading-section">
          <h3>Editions on your shelves</h3>
          <BarList
            label="Books by edition"
            bars={insights.formats.map((f) => ({
              key: f.name,
              label: f.name,
              value: f.books,
            }))}
          />
          {insights.shelves.length > 0 && (
            <>
              <h4>Your shelves</h4>
              <BarList
                label="Books on your own shelves"
                bars={insights.shelves.map((s) => ({
                  key: s.name,
                  label: s.name,
                  value: s.books,
                }))}
              />
            </>
          )}
        </section>
      </div>

      {insights.rereads.length > 0 && (
        <section className="reading-section">
          <h3>
            Read more than once{" "}
            <span className="muted">{insights.rereads.length}</span>
          </h3>
          <ul className="stat-list">
            {insights.rereads
              .map((id) => byId.get(id))
              .filter((b): b is InsightBook => !!b)
              .map((b) => (
                <li key={b.goodreads_id} className="stat-row compact">
                  <span className="stat-book">
                    <span className="stat-title">{b.title}</span>
                    {b.authors[0] && (
                      <span className="muted"> · {b.authors[0]}</span>
                    )}
                  </span>
                  <span className="muted stat-num">{b.read_count}×</span>
                </li>
              ))}
          </ul>
        </section>
      )}

      <GoodreadsTable
        books={insights.books}
        filter={filter}
        onFilter={setFilter}
      />
    </div>
  );
}

function ratingGap(gap: { books: number; mine: number; goodreads: number }) {
  const diff = gap.mine - gap.goodreads;
  const over = `over ${num(gap.books)} books`;
  if (Math.abs(diff) < 0.1)
    return `You rate about the same as other Goodreads readers, ${over}.`;
  return `You rate ${Math.abs(diff).toFixed(1)} stars ${diff < 0 ? "lower" : "higher"} than other Goodreads readers on average, ${over}.`;
}

function seriesNote(s: GoodreadsInsights["series"][number]) {
  const list = (ns: (string | number)[]) => ns.map((n) => `#${n}`).join(", ");
  const parts = [`Read ${list(s.read)}`];
  if (s.unread.length) parts.push(`on your shelves ${list(s.unread)}`);
  if (s.missing.length) parts.push(`${list(s.missing)} not on your shelves`);
  return parts.join(" · ");
}
