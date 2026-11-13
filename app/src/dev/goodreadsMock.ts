// Goodreads insights for the browser mock: a made-up export over the sample
// library and some invented books, summed up roughly the way
// crates/core/src/goodreads_insights.rs does. Presentation only; the Rust
// tests check the real numbers.

import type { Book, GoodreadsBook, GoodreadsInsights } from "../api";

const INVENTED: [string, string, string | null][] = [
  ["The Lantern Keepers", "Mira Okafor", "Lanterns, #1"],
  ["The Ember Road", "Mira Okafor", "Lanterns, #2"],
  ["The Last Lantern", "Mira Okafor", "Lanterns, #4"],
  ["Salt and Signal", "Tomas Reyes", null],
  ["Quiet Orbits", "Tomas Reyes", null],
  ["A Field Guide to Small Gods", "Hollis Grey", null],
  ["The Cartographer's Daughter", "Hollis Grey", null],
  ["Winter in the Archive", "June Abernathy", null],
  ["Paper Moons", "June Abernathy", null],
  ["Ninefold", "Idris Vance", "Ninefold, #1"],
  ["Tenfold", "Idris Vance", "Ninefold, #2"],
  ["The Tidewright", "Sora Lindqvist", null],
  ["Letters from the Floodplain", "Ada Mensah", null],
  ["A Brief History of Rain", "Ada Mensah", null],
  ["The Clockmaker's Garden", "Peregrine Holt", null],
  ["Static Hearts", "Lena Marsh", null],
  ["Bright Harbour", "Lena Marsh", null],
  ["The Long Quiet", "Wren Castillo", null],
  ["Under the Copper Sky", "Wren Castillo", null],
  ["How to Read a River", "Dev Raman", null],
  ["The Night Ferry", "Noor Haddad", null],
  ["The Map of Lost Things", "Noor Haddad", null],
  ["Glasswing", "Elias Brandt", "Glasswing, #1"],
  ["Ironwing", "Elias Brandt", "Glasswing, #2"],
  ["Small Fires", "Ruth Calder", null],
  ["The Orchard at World's End", "Ruth Calder", null],
  ["Northern Lights, Southern Seas", "Kit Mbeki", null],
  ["The Unfinished Atlas", "Kit Mbeki", null],
];

const BINDINGS = [
  "Paperback",
  "Hardcover",
  "Kindle Edition",
  "Mass Market Paperback",
  "Paperback",
  "ebook",
];

/** A small repeatable random sequence, so the mock looks the same each load. */
function random(seed: number) {
  let s = seed;
  return () => {
    s = (s * 1103515245 + 12345) % 2147483648;
    return s / 2147483648;
  };
}

const iso = (d: Date) => d.toISOString().slice(0, 10);

function splitSeries(raw: string | null) {
  if (!raw) return null;
  const [name, number] = raw.split(", #");
  return { name, number };
}

function makeBooks(library: Book[], today: Date): GoodreadsBook[] {
  const rnd = random(7);
  const fromLibrary = library
    .filter((b) => b.kind === "book")
    .map((b) => [
      b.title,
      b.authors[0] ?? "Anonymous",
      b.series ? `${b.series}, #${b.series_index ?? 1}` : null,
    ]) as [string, string, string | null][];
  return [...fromLibrary, ...INVENTED].map(([title, author, series], i) => {
    const r = rnd();
    const shelf: GoodreadsBook["shelf"] =
      i === 3 ? "currently_reading" : r < 0.55 ? "read" : "to_read";
    const added = new Date(today);
    added.setDate(added.getDate() - Math.round(30 + rnd() * 1500));
    let dateRead: string | null = null;
    if (shelf === "read" && rnd() < 0.8) {
      const read = new Date(added);
      read.setDate(read.getDate() + Math.round(rnd() * 400));
      dateRead = iso(read > today ? today : read);
    }
    const rating =
      shelf === "read" && rnd() < 0.8
        ? Math.min(5, 2 + Math.round(rnd() * 3))
        : null;
    const shelves = [
      {
        name:
          shelf === "to_read"
            ? "to-read"
            : shelf === "read"
              ? "read"
              : "currently-reading",
        position: null,
      },
    ];
    if (rating === 5) shelves.push({ name: "favorites", position: null });
    if (rnd() < 0.3) shelves.push({ name: "sci-fi", position: null });
    return {
      goodreads_id: String(1000 + i),
      title,
      series: splitSeries(series),
      authors: [author],
      isbn: null,
      isbn13: null,
      publisher: i % 3 ? "Penguin" : "Orbit",
      binding: BINDINGS[i % BINDINGS.length],
      pages: 150 + Math.round(rnd() * 650),
      year_published: 1990 + Math.round(rnd() * 35),
      original_year: i % 7 === 0 ? 1890 + i : 1950 + Math.round(rnd() * 75),
      rating,
      shelf,
      shelves,
      date_read: dateRead,
      date_added: iso(added),
      read_count: shelf === "read" ? (i === 5 ? 3 : 1) : 0,
      review:
        rating === 5
          ? "Loved it. The ending stayed with me for days.\nWould read again."
          : null,
    };
  });
}

const days = (a: string, b: Date | string) =>
  Math.round((new Date(b).getTime() - new Date(a).getTime()) / 86_400_000);

const median = (v: number[]) => {
  if (!v.length) return null;
  const s = [...v].sort((a, b) => a - b);
  const m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : Math.floor((s[m - 1] + s[m]) / 2);
};

const avg = (v: number[]) =>
  v.length ? v.reduce((a, b) => a + b, 0) / v.length : null;

const counted = (m: Map<string, number>) =>
  [...m]
    .map(([name, books]) => ({ name, books }))
    .sort((a, b) => b.books - a.books || a.name.localeCompare(b.name));

export function mockInsights(
  library: Book[],
  today = new Date(),
): GoodreadsInsights {
  const books = makeBooks(library, today);
  const titleKey = (t: string) => t.toLowerCase();
  const libByTitle = new Map(library.map((b) => [titleKey(b.title), b.id]));
  const read = books.filter((b) => b.shelf === "read");
  const toRead = books.filter((b) => b.shelf === "to_read");
  const inLib = (b: GoodreadsBook) => libByTitle.has(titleKey(b.title));
  const year = today.getFullYear();

  const byYear = new Map<number, { books: number; pages: number }>();
  for (const b of read)
    if (b.date_read) {
      const y = Number(b.date_read.slice(0, 4));
      const e = byYear.get(y) ?? { books: 0, pages: 0 };
      e.books++;
      e.pages += b.pages ?? 0;
      byYear.set(y, e);
    }
  const first = Math.min(...byYear.keys());
  const years = [];
  for (let y = first; y <= year; y++)
    years.push({ year: y, ...(byYear.get(y) ?? { books: 0, pages: 0 }) });

  const md = (d: string) => d.slice(5);
  const todayMd = iso(today).slice(5);
  const dated = read.flatMap((b) => (b.date_read ? [b.date_read] : []));
  const lastYear = dated.filter((d) => d.startsWith(String(year - 1)));
  const stars: [number, number, number, number, number] = [0, 0, 0, 0, 0];
  for (const b of read) if (b.rating) stars[b.rating - 1]++;
  const readLastYear = dated.filter((d) => days(d, today) < 365).length;

  const waits = read.flatMap((b) =>
    b.date_read && b.date_added && days(b.date_added, b.date_read) > 0
      ? [{ id: b.goodreads_id, d: days(b.date_added, b.date_read) }]
      : [],
  );
  const longest = waits.reduce((a, b) => (b.d > a.d ? b : a), waits[0]);

  const authors = new Map<string, GoodreadsBook[]>();
  for (const b of books)
    authors.set(b.authors[0], [...(authors.get(b.authors[0]) ?? []), b]);

  const series = new Map<string, GoodreadsBook[]>();
  for (const b of books)
    if (b.series)
      series.set(b.series.name, [...(series.get(b.series.name) ?? []), b]);

  const decades = new Map<number | null, { read: number; to_read: number }>();
  for (const b of books) {
    const y = b.original_year ?? b.year_published;
    if (y == null || b.shelf === "currently_reading") continue;
    const k = y >= 1900 ? y - (y % 10) : null;
    const e = decades.get(k) ?? { read: 0, to_read: 0 };
    if (b.shelf === "read") e.read++;
    else e.to_read++;
    decades.set(k, e);
  }

  const edges = [0, 200, 300, 400, 500, 700];
  const buckets = edges.map((min, i) => ({
    min,
    max: edges[i + 1] ?? null,
    read: 0,
    to_read: 0,
  }));
  for (const b of books) {
    if (b.pages == null) continue;
    const i = edges.filter((e) => b.pages! >= e).length - 1;
    if (b.shelf === "read") buckets[i].read++;
    else if (b.shelf === "to_read") buckets[i].to_read++;
  }
  const paged = read.filter((b) => b.pages != null);
  const byPages = [...paged].sort((a, b) => a.pages! - b.pages!);

  const kinds = new Map<string, number>();
  for (const b of books) {
    const k = /kindle|ebook/i.test(b.binding ?? "")
      ? "Ebook"
      : /paperback/i.test(b.binding ?? "")
        ? "Paperback"
        : "Hardcover";
    kinds.set(k, (kinds.get(k) ?? 0) + 1);
  }
  const shelves = new Map<string, number>();
  for (const b of books)
    for (const s of b.shelves)
      if (!["read", "to-read", "currently-reading"].includes(s.name))
        shelves.set(s.name, (shelves.get(s.name) ?? 0) + 1);

  const oldest = [...toRead].sort((a, b) =>
    (a.date_added ?? "").localeCompare(b.date_added ?? ""),
  )[0];

  return {
    import: {
      imported_at: new Date(today.getTime() - 2 * 86_400_000).toISOString(),
      file_name: "goodreads_library_export.csv",
      file_modified: new Date(today.getTime() - 3 * 86_400_000).toISOString(),
      latest_activity: iso(new Date(today.getTime() - 4 * 86_400_000)),
      books: books.length,
      added: books.length,
      updated: 0,
    },
    imports: 2,
    totals: {
      books: books.length,
      read: read.length,
      to_read: toRead.length,
      reading: books.filter((b) => b.shelf === "currently_reading").length,
      other: 0,
      rated: read.filter((b) => b.rating).length,
      reviewed: books.filter((b) => b.review).length,
      pages_read: read.reduce((a, b) => a + (b.pages ?? 0), 0),
      read_undated: read.filter((b) => !b.date_read).length,
      in_library: books.filter(inLib).length,
    },
    years,
    pace: {
      year,
      this_year: dated.filter((d) => d.startsWith(String(year))).length,
      last_year_by_now: lastYear.filter((d) => md(d) <= todayMd).length,
      last_year: lastYear.length,
    },
    ratings: {
      stars,
      unrated: read.filter((b) => !b.rating).length,
      average: avg(read.flatMap((b) => (b.rating ? [b.rating] : []))),
      against_goodreads: null,
    },
    to_read: {
      books: toRead.length,
      in_library: toRead.filter(inLib).length,
      pages: toRead.reduce((a, b) => a + (b.pages ?? 0), 0),
      median_age_days: median(
        toRead.flatMap((b) =>
          b.date_added ? [days(b.date_added, today)] : [],
        ),
      ),
      oldest: oldest?.goodreads_id ?? null,
      years_to_clear: readLastYear ? toRead.length / readLastYear : null,
      read_last_year: readLastYear,
    },
    wait: longest
      ? {
          books: waits.length,
          median_days: median(waits.map((w) => w.d))!,
          longest: longest.id,
          longest_days: longest.d,
          logged_after: read.length - waits.length,
        }
      : null,
    authors: [...authors]
      .map(([name, list]) => {
        const r = list.filter((b) => b.shelf === "read");
        return {
          name,
          read: r.length,
          books: list.length,
          average: avg(r.flatMap((b) => (b.rating ? [b.rating] : []))),
        };
      })
      .filter((a) => a.read >= 2)
      .sort((a, b) => b.read - a.read || b.books - a.books)
      .slice(0, 10),
    series: [...series]
      .map(([name, list]) => ({
        name,
        author: list[0].authors[0],
        read: list
          .filter((b) => b.shelf === "read")
          .map((b) => b.series!.number),
        unread: list
          .filter((b) => b.shelf !== "read")
          .map((b) => b.series!.number),
        missing: name === "Lanterns" ? [3] : [],
      }))
      .filter((s) => s.read.length && (s.unread.length || s.missing.length)),
    decades: [...decades]
      .sort(([a], [b]) => (a ?? -1) - (b ?? -1))
      .map(([decade, c]) => ({ decade, ...c })),
    lengths: {
      buckets,
      average: paged.length
        ? Math.round(paged.reduce((a, b) => a + b.pages!, 0) / paged.length)
        : null,
      longest: byPages.at(-1)?.goodreads_id ?? null,
      shortest: byPages[0]?.goodreads_id ?? null,
    },
    formats: counted(kinds),
    shelves: counted(shelves),
    rereads: read.filter((b) => b.read_count > 1).map((b) => b.goodreads_id),
    books: books.map((b) => ({
      ...b,
      library: libByTitle.has(titleKey(b.title))
        ? [libByTitle.get(titleKey(b.title))!]
        : [],
    })),
  };
}

/** What the app shows before any export was imported. */
export function emptyInsights(): GoodreadsInsights {
  return {
    import: null,
    imports: 0,
    totals: {
      books: 0,
      read: 0,
      to_read: 0,
      reading: 0,
      other: 0,
      rated: 0,
      reviewed: 0,
      pages_read: 0,
      read_undated: 0,
      in_library: 0,
    },
    years: [],
    pace: null,
    ratings: {
      stars: [0, 0, 0, 0, 0],
      unrated: 0,
      average: null,
      against_goodreads: null,
    },
    to_read: {
      books: 0,
      in_library: 0,
      pages: 0,
      median_age_days: null,
      oldest: null,
      years_to_clear: null,
      read_last_year: 0,
    },
    wait: null,
    authors: [],
    series: [],
    decades: [],
    lengths: { buckets: [], average: null, longest: null, shortest: null },
    formats: [],
    shelves: [],
    rereads: [],
    books: [],
  };
}
