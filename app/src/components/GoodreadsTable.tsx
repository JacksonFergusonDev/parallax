import { ChevronRight, Library } from "lucide-react";
import { Fragment, useMemo, useState } from "react";
import {
  dateFmt,
  firstDir,
  rowDate,
  statusLabel,
  tableRows,
  type InsightBook,
  type SortKey,
  type StatusFilter,
  type TableFilter,
  type TableSort,
} from "../goodreadsInsights";
import { num } from "./charts";

/** Rows shown before "Show all". */
export const COLLAPSED_ROWS = 25;

const STATUSES: [StatusFilter, string][] = [
  ["all", "All"],
  ["read", "Read"],
  ["to_read", "To read"],
  ["currently_reading", "Reading"],
];

const COLUMNS: [SortKey, string][] = [
  ["title", "Title"],
  ["author", "Author"],
  ["status", "Status"],
  ["rating", "Rating"],
  ["date", "Read or added"],
  ["pages", "Pages"],
  ["year", "Published"],
];

interface Props {
  books: InsightBook[];
  filter: TableFilter;
  onFilter: (filter: TableFilter) => void;
}

/** Every saved book, searchable and sortable, each opening to its details. */
export function GoodreadsTable({ books, filter, onFilter }: Props) {
  const { status, inLibrary } = filter;
  const setStatus = (s: StatusFilter) => onFilter({ ...filter, status: s });
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<TableSort>({ key: "date", dir: "desc" });
  const [all, setAll] = useState(false);
  const [open, setOpen] = useState<Set<string>>(new Set());

  const rows = useMemo(
    () => tableRows(books, filter, query, sort),
    [books, filter, query, sort],
  );
  const shown = all ? rows : rows.slice(0, COLLAPSED_ROWS);
  const count = (s: StatusFilter) =>
    s === "all" ? books.length : books.filter((b) => b.shelf === s).length;

  const toggle = (id: string) =>
    setOpen((o) => {
      const next = new Set(o);
      if (!next.delete(id)) next.add(id);
      return next;
    });
  const sortBy = (key: SortKey) =>
    setSort((s) =>
      s.key === key
        ? { key, dir: s.dir === "asc" ? "desc" : "asc" }
        : { key, dir: firstDir(key) },
    );

  return (
    <section className="reading-section" id="goodreads-books">
      <div className="section-head gr-table-head">
        <h3>
          All books <span className="muted">{num(books.length)}</span>
        </h3>
        <input
          type="search"
          className="gr-search"
          placeholder="Search titles, authors, series"
          aria-label="Search your Goodreads books"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>
      <div className="segmented gr-status" role="group" aria-label="Status">
        {STATUSES.filter(([s]) => s === "all" || count(s) > 0).map(
          ([s, label]) => (
            <button
              key={s}
              aria-pressed={status === s}
              className={status === s ? "active" : ""}
              onClick={() => setStatus(s)}
            >
              {label} <span className="muted">{num(count(s))}</span>
            </button>
          ),
        )}
      </div>
      {books.some((b) => b.library.length > 0) && (
        <label className="gr-owned">
          <input
            type="checkbox"
            checked={inLibrary}
            onChange={(e) =>
              onFilter({ ...filter, inLibrary: e.target.checked })
            }
          />
          Only books in your library
        </label>
      )}
      <table className="gr-table">
        <thead>
          <tr>
            {COLUMNS.map(([key, label]) => (
              <th
                key={key}
                data-column={key}
                aria-sort={
                  sort.key === key
                    ? sort.dir === "asc"
                      ? "ascending"
                      : "descending"
                    : undefined
                }
              >
                <button onClick={() => sortBy(key)}>
                  {label}
                  {sort.key === key && (
                    <span aria-hidden> {sort.dir === "asc" ? "↑" : "↓"}</span>
                  )}
                </button>
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {shown.map((b) => {
            const expanded = open.has(b.goodreads_id);
            return (
              <Fragment key={b.goodreads_id}>
                <tr className={expanded ? "open" : ""}>
                  <td data-column="title">
                    <button
                      className="gr-expand"
                      aria-expanded={expanded}
                      onClick={() => toggle(b.goodreads_id)}
                    >
                      <ChevronRight
                        size={16}
                        aria-hidden
                        className="gr-chevron"
                      />
                      <span className="gr-title">
                        <span className="stat-title">{b.title}</span>
                        {b.series && (
                          <span className="muted">
                            {" "}
                            ({b.series.name} #{b.series.number})
                          </span>
                        )}
                      </span>
                      {b.library.length > 0 && (
                        <Library
                          size={16}
                          className="gr-in-library"
                          aria-label="In your library"
                        />
                      )}
                    </button>
                  </td>
                  <td data-column="author" className="muted">
                    {b.authors[0]}
                  </td>
                  <td data-column="status">{statusLabel(b.shelf)}</td>
                  <td
                    data-column="rating"
                    aria-label={b.rating ? `${b.rating} stars` : "Not rated"}
                  >
                    {b.rating ? (
                      <span className="gr-stars">{"★".repeat(b.rating)}</span>
                    ) : (
                      <span className="muted">—</span>
                    )}
                  </td>
                  <td data-column="date" className="muted">
                    {dateFmt(rowDate(b))}
                    {!b.date_read && b.date_added && " (added)"}
                  </td>
                  <td data-column="pages" className="num">
                    {b.pages ? num(b.pages) : ""}
                  </td>
                  <td data-column="year" className="num muted">
                    {yearLabel(b.original_year ?? b.year_published)}
                  </td>
                </tr>
                {expanded && (
                  <tr className="gr-detail">
                    <td colSpan={COLUMNS.length}>
                      <BookDetails book={b} />
                    </td>
                  </tr>
                )}
              </Fragment>
            );
          })}
        </tbody>
      </table>
      {rows.length === 0 && (
        <p className="muted reading-note">No books match.</p>
      )}
      {rows.length > COLLAPSED_ROWS && (
        <button className="secondary gr-more" onClick={() => setAll((a) => !a)}>
          {all ? "Show fewer" : `Show all ${num(rows.length)}`}
        </button>
      )}
    </section>
  );
}

const yearLabel = (y: number | null) =>
  y === null ? "" : y < 0 ? `${-y} BC` : String(y);

function BookDetails({ book: b }: { book: InsightBook }) {
  const own = b.shelves
    .map((s) => s.name)
    .filter((n) => !["read", "to-read", "currently-reading"].includes(n));
  const facts: [string, string][] = [];
  if (b.authors.length > 1) facts.push(["With", b.authors.slice(1).join(", ")]);
  if (b.date_read) facts.push(["Read", dateFmt(b.date_read)]);
  if (b.date_added) facts.push(["Added", dateFmt(b.date_added)]);
  if (b.read_count > 1) facts.push(["Times read", String(b.read_count)]);
  if (own.length) facts.push(["Your shelves", own.join(", ")]);
  if (b.average_rating)
    facts.push(["Goodreads average", `${b.average_rating.toFixed(2)} ★`]);
  const edition = [b.binding, b.publisher, b.year_published]
    .filter(Boolean)
    .join(", ");
  if (edition) facts.push(["Edition", edition]);
  if (b.isbn13 || b.isbn) facts.push(["ISBN", (b.isbn13 ?? b.isbn)!]);
  if (b.library.length)
    facts.push([
      "In your library",
      b.library.length === 1 ? "Yes" : `${b.library.length} copies`,
    ]);
  return (
    <div className="gr-details">
      <dl>
        {facts.map(([k, v]) => (
          <Fragment key={k}>
            <dt>{k}</dt>
            <dd>{v}</dd>
          </Fragment>
        ))}
      </dl>
      {b.review && (
        <blockquote className="gr-review">
          {b.review
            .split("\n")
            .filter((line) => line.trim())
            .map((line, i) => (
              <p key={i}>{line}</p>
            ))}
        </blockquote>
      )}
    </div>
  );
}
