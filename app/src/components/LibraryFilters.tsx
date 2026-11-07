import {
  isFiltered,
  NO_FILTER,
  NONE,
  type Counted,
  type Facets,
  type KoboFilter,
  type LibraryFilter,
  type ReadingFilter,
} from "../libraryFilter";

interface Props {
  filter: LibraryFilter;
  facets: Facets;
  /** A Kobo is connected, so its status and collections can be filtered. */
  connected: boolean;
  onChange: (filter: LibraryFilter) => void;
}

const READING: [ReadingFilter, string][] = [
  ["unread", "Unread"],
  ["reading", "Reading"],
  ["finished", "Finished"],
];

function FilterSelect({
  label,
  all,
  none,
  noneCount,
  value,
  options,
  onChange,
}: {
  label: string;
  all: string;
  /** The option for books with none of these. */
  none: string;
  noneCount: number;
  value: string | null;
  options: readonly Counted[];
  onChange: (value: string | null) => void;
}) {
  return (
    <select
      className={`sort-select filter-select ${value === null ? "" : "active"}`}
      aria-label={label}
      value={value ?? ""}
      onChange={(e) => onChange(e.target.value || null)}
    >
      <option value="">{all}</option>
      {(noneCount > 0 || value === NONE) && (
        <option value={NONE}>
          {none} ({noneCount})
        </option>
      )}
      {options.map(([name, count]) => (
        <option key={name} value={name}>
          {name} ({count})
        </option>
      ))}
    </select>
  );
}

/** Narrows the library: what's on the Kobo, then collection, genre, status. */
export function LibraryFilters({ filter, facets, connected, onChange }: Props) {
  const kobo: [KoboFilter, string, number][] = [
    ["all", "All", facets.kobo.all],
    ["on", "On Kobo", facets.kobo.on],
    ["off", "Not on Kobo", facets.kobo.off],
  ];
  if (facets.kobo.stale > 0 || filter.kobo === "stale") {
    kobo.push(["stale", "Update available", facets.kobo.stale]);
  }
  return (
    <div className="filter-bar" role="group" aria-label="Filter books">
      {connected && (
        <>
          <div className="segmented">
            {kobo.map(([value, label, count]) => (
              <button
                key={value}
                className={filter.kobo === value ? "active" : ""}
                aria-pressed={filter.kobo === value}
                onClick={() => onChange({ ...filter, kobo: value })}
              >
                {label} <span className="filter-count">{count}</span>
              </button>
            ))}
          </div>
          <FilterSelect
            label="Filter by collection"
            all="All collections"
            none="In no collection"
            noneCount={facets.noCollection}
            value={filter.collection}
            options={facets.collections}
            onChange={(collection) => onChange({ ...filter, collection })}
          />
        </>
      )}
      <FilterSelect
        label="Filter by genre"
        all="All genres"
        none="No genre"
        noneCount={facets.noGenre}
        value={filter.genre}
        options={facets.genres}
        onChange={(genre) => onChange({ ...filter, genre })}
      />
      <select
        className={`sort-select filter-select ${filter.reading ? "active" : ""}`}
        aria-label="Filter by reading status"
        value={filter.reading ?? ""}
        onChange={(e) =>
          onChange({
            ...filter,
            reading: (e.target.value || null) as ReadingFilter | null,
          })
        }
      >
        <option value="">Any status</option>
        {READING.map(([value, label]) => (
          <option key={value} value={value}>
            {label} ({facets.reading[value]})
          </option>
        ))}
      </select>
      {isFiltered(filter) && (
        <button className="link" onClick={() => onChange(NO_FILTER)}>
          Clear filters
        </button>
      )}
    </div>
  );
}
