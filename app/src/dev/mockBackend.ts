// Browser-only stand-in for the Rust backend, so the UI can be developed and
// screenshotted with `pnpm dev` in a normal browser. Loaded only in dev builds
// when not running inside Tauri; never part of the app bundle.
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Book, DeviceBook, DeviceInfo, MetadataEdit, Rule } from "../api";

const PALETTE = [
  "#2f6f5e",
  "#8a3b2e",
  "#2e4a8a",
  "#7a5c1e",
  "#5b2e7a",
  "#1e6a7a",
];

function coverSvg(title: string, author: string, i: number): string {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="400" height="600">
    <rect width="400" height="600" fill="${PALETTE[i % PALETTE.length]}"/>
    <text x="32" y="260" font-family="Georgia" font-size="40" fill="#fff">${title.slice(0, 16)}</text>
    <text x="32" y="320" font-family="Georgia" font-size="22" fill="#fffc">${author}</text></svg>`;
  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`;
}

const SAMPLE: [string, string, string | null, string | null, string[]][] = [
  [
    "The Lies of Locke Lamora",
    "Scott Lynch",
    "Gentleman Bastard",
    "1",
    ["Fantasy"],
  ],
  [
    "Red Seas Under Red Skies",
    "Scott Lynch",
    "Gentleman Bastard",
    "2",
    ["Fantasy"],
  ],
  ["Dune", "Frank Herbert", "Dune Chronicles", "1", ["Sci-Fi"]],
  ["Paradise Lost", "John Milton", null, null, ["Poetry", "Classics"]],
  ["Pride and Prejudice", "Jane Austen", null, null, ["Classics"]],
  [
    "We Are Legion (We Are Bob)",
    "Dennis E. Taylor",
    "Bobiverse",
    "1",
    ["Sci-Fi"],
  ],
  ["Warbreaker", "Brandon Sanderson", null, null, ["Fantasy"]],
  ["A Book Without a Cover", "Anonymous", null, null, []],
];

let books: Book[] = SAMPLE.map(([title, author, series, index, tags], i) => ({
  id: i + 1,
  title,
  authors: [author],
  series,
  series_index: index,
  tags,
  language: "en",
  publisher: i % 2 ? "Penguin" : null,
  published: i % 3 ? `${1965 + i * 4}-08-15T04:00:00+00:00` : null,
  description:
    i === 0
      ? `<p>${"A sweeping tale of politics, prophecy and desert survival. ".repeat(8)}</p><p>Now <i>with</i> a new introduction.</p>`
      : null,
  identifiers: i === 0 ? [{ scheme: "isbn", value: "9780441013593" }] : [],
  words: 60_000 + i * 21_000,
  pages: 200 + i * 70,
  pages_estimated: i % 4 !== 1,
  original_name: `${title}.epub`,
  file_size: 1_000_000,
  has_cover: i < SAMPLE.length - 1,
  custom_cover: false,
  cover_upscaled: false,
  added_at: "2026-10-31T00:00:00Z",
  modified_at: "2026-10-31T00:00:00Z",
  cover: i < SAMPLE.length - 1 ? coverSvg(title, author, i) : null,
  contributors: i === 5 ? [{ name: "Some Translator", role: "trl" }] : [],
  annotations: i === 2 ? 3 : 0,
  reading:
    i === 2
      ? {
          device_serial: "N000000000001",
          status: "reading" as const,
          percent: 42,
          last_read: "2026-10-29T20:00:00Z",
          time_spent_secs: 7200,
          seen_at: "2026-10-31T00:00:00Z",
        }
      : i === 4
        ? {
            device_serial: "N000000000001",
            status: "finished" as const,
            percent: 100,
            last_read: "2026-09-01T20:00:00Z",
            time_spent_secs: 30000,
            seen_at: "2026-10-31T00:00:00Z",
          }
        : null,
}));

// A second copy of Dune, for duplicate review.
books.push({
  ...books.find((b) => b.title === "Dune")!,
  id: books.length + 1,
  title: "Dune: Deluxe Edition",
  series: null,
  series_index: null,
  tags: [],
  original_name: "Dune Deluxe.epub",
  file_size: 3_400_000,
  added_at: "2026-09-01T00:00:00Z",
  modified_at: "2026-09-01T00:00:00Z",
});

const device: DeviceInfo = {
  kind: "kobo",
  model: "Kobo Libra 2",
  firmware: "4.38.23697",
  serial: "N000000000001",
  mount: "/Volumes/KOBOeReader",
  capabilities: {
    native_collections: true,
    series: true,
    reading_progress: true,
    reading_time: true,
  },
  // `?readonly` previews a Kobo the app won't change.
  read_only: new URLSearchParams(location.search).has("readonly")
    ? "the Tolino Shine Color runs Tolino's version of Kobo's software, which stores data differently"
    : null,
};
const dismissed = new Set<string>();
const settings = new Map<string, string>();
const health = { missing: true, duplicates: true, images: 23 };
const onDevice = new Map<number, string>([
  [3, "2026-10-01T00:00:00Z"],
  [5, "2026-10-01T00:00:00Z"],
]);
const shelves = new Map<string, Set<string>>([["Favourites", new Set()]]);
// `?shelves=N` adds N collections, some with long names, to check the sidebar.
for (
  let i = 1;
  i <= Number(new URLSearchParams(location.search).get("shelves"));
  i++
)
  shelves.set(
    i % 5
      ? `Collection ${i}`
      : `27 of 1987 Locus Award for All-Time Best Fantasy Novel ${i}`,
    new Set(),
  );
const shelvesOf = (id: string) =>
  [...shelves].filter(([, ids]) => ids.has(id)).map(([n]) => n);
let rules: Rule[] = [
  { id: 1, kind: "series", enabled: true, min_books: 2, prefix: "" },
];
let nextRuleId = 2;
/** Books on the mock device with what rules look at. */
const ruleItems = () => [
  ...books
    .filter((b) => onDevice.has(b.id))
    .map((b) => ({
      id: contentId(b.id),
      title: b.title,
      series: b.series,
      authors: b.authors,
      tags: b.tags,
    })),
  ...storeBooks.map((b) => ({
    id: b.id,
    title: b.title,
    series: b.series,
    authors: b.author ? [b.author] : [],
    tags: [],
  })),
];
function mockPlan() {
  const groups = new Map<string, { ids: string[]; titles: string[] }>();
  for (const r of rules.filter((x) => x.enabled)) {
    const byKey = new Map<string, typeof items>();
    const items = ruleItems();
    for (const it of items) {
      const keys =
        r.kind === "series"
          ? it.series
            ? [it.series]
            : []
          : r.kind === "tag"
            ? it.tags
            : it.authors;
      for (const k of keys) byKey.set(k, [...(byKey.get(k) ?? []), it]);
    }
    for (const [k, members] of byKey) {
      if (members.length < r.min_books) continue;
      const name = r.prefix + k;
      const g = groups.get(name) ?? { ids: [], titles: [] };
      for (const m of members)
        if (!shelves.get(name)?.has(m.id) && !g.ids.includes(m.id)) {
          g.ids.push(m.id);
          g.titles.push(m.title);
        }
      groups.set(name, g);
    }
  }
  return [...groups]
    .filter(([, g]) => g.ids.length)
    .map(([name, g]) => ({
      name,
      new: !shelves.has(name),
      content_ids: g.ids,
      titles: g.titles,
    }));
}
const contentId = (id: number) =>
  `file:///mnt/onboard/KEpub/book-${id}.kepub.epub`;
let storeBooks: DeviceBook[] = [
  ["Endurance", "Alfred Lansing", "reading", 63, "2026-10-29T20:00:00Z"],
  ["What If?", "Randall Munroe", "reading", 8, "2024-10-11T10:00:00Z"],
  [
    "We Are Legion (We Are Bob)",
    "Dennis E. Taylor",
    "finished",
    100,
    "2026-07-13T10:00:00Z",
  ],
  ["Underland", "Robert Macfarlane", "unread", 0, null],
].map(([title, author, status, pct, last], i) => ({
  id: `file:///mnt/onboard/KEpub/Nonfiction/${title}.kepub.epub`,
  title: title as string,
  author: author as string,
  path: `KEpub/Nonfiction/${title}.kepub.epub`,
  sideloaded: true,
  series: i === 2 ? "Bobiverse" : null,
  series_number: i === 2 ? "1" : null,
  read_status: status as DeviceBook["read_status"],
  percent_read: pct as number,
  last_read: last as string | null,
  reading_time_secs: null,
  added: "2024-10-09T15:27:08Z",
  file_size: 1_000_000,
  collections: [],
}));
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
let upscaleCancelled = false;

export function installMockBackend() {
  mockWindows("main");
  // Cover "paths" are already data URLs here.
  (
    window as unknown as {
      __TAURI_INTERNALS__: { convertFileSrc: (p: string) => string };
    }
  ).__TAURI_INTERNALS__.convertFileSrc = (p) => p;

  mockIPC(
    async (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "list_devices":
          return [device];
        case "library_list":
          return books;
        case "books_on_device":
          return [...onDevice].map(([id, sentAt]) => ({
            book_id: id,
            device_path: `KEpub/book-${id}.kepub.epub`,
            content_id: contentId(id),
            sent_at: sentAt,
            stale: books.find((b) => b.id === id)!.modified_at > sentAt,
          }));
        case "device_books":
          await sleep(300);
          return [
            ...books
              .filter((b) => onDevice.has(b.id))
              .map((b): DeviceBook => ({
                id: contentId(b.id),
                title: b.title,
                author: b.authors.join(", "),
                path: `KEpub/book-${b.id}.kepub.epub`,
                sideloaded: true,
                series: b.series,
                series_number: b.series_index,
                read_status: "unread",
                percent_read: 0,
                last_read: null,
                reading_time_secs: null,
                added: onDevice.get(b.id)!,
                file_size: b.file_size,
                collections: [],
              })),
            ...storeBooks,
          ]
            .map((b) => ({ ...b, collections: shelvesOf(b.id) }))
            .sort((x, y) => x.title.localeCompare(y.title));
        case "remove_from_device": {
          const ids = a.contentIds as string[];
          if (ids.some((id) => id.includes("Old Book"))) health.missing = false;
          storeBooks = storeBooks.filter((b) => !ids.includes(b.id));
          for (const id of [...onDevice.keys()])
            if (ids.includes(contentId(id))) onDevice.delete(id);
          return ids.map((content_id) => ({
            content_id,
            result: { Ok: null },
          }));
        }
        case "list_collections":
          return [...shelves]
            .map(([name, ids]) => ({ name, book_count: ids.size }))
            .sort((x, y) => x.name.localeCompare(y.name));
        case "folder_groups":
          return [
            { folder: "Nonfiction", content_ids: storeBooks.map((b) => b.id) },
            { folder: "Classics", content_ids: [contentId(5)] },
          ];
        case "reconcile_device":
          return { series: 0, grouped: 0 };
        case "rules_list":
          return rules;
        case "rules_save": {
          const rule = { ...(a.rule as Rule) };
          if (rule.id === 0) rule.id = nextRuleId++;
          rules = [...rules.filter((r) => r.id !== rule.id), rule].sort(
            (x, y) => x.id - y.id,
          );
          return rule;
        }
        case "rules_delete":
          rules = rules.filter((r) => r.id !== a.id);
          return null;
        case "rules_preview":
          return mockPlan();
        case "rules_apply": {
          const plan = mockPlan();
          for (const p of plan) {
            const set = shelves.get(p.name) ?? new Set<string>();
            p.content_ids.forEach((id) => set.add(id));
            shelves.set(p.name, set);
          }
          return plan.reduce((n, p) => n + p.content_ids.length, 0);
        }
        case "change_collections": {
          type Change = {
            kind: string;
            name?: string;
            from?: string;
            to?: string;
            content_ids?: string[];
          };
          for (const c of a.changes as Change[]) {
            if (c.kind === "create")
              shelves.set(c.name!, shelves.get(c.name!) ?? new Set());
            if (c.kind === "delete") shelves.delete(c.name!);
            if (c.kind === "rename") {
              shelves.set(c.to!, shelves.get(c.from!) ?? new Set());
              shelves.delete(c.from!);
            }
            if (c.kind === "add") {
              const set = shelves.get(c.name!) ?? new Set();
              c.content_ids!.forEach((id) => set.add(id));
              shelves.set(c.name!, set);
            }
            if (c.kind === "remove")
              c.content_ids!.forEach((id) => shelves.get(c.name!)?.delete(id));
          }
          return null;
        }
        case "tidy_device":
          return 68;
        case "device_history": {
          const stamp = (at: string, v: string) => ({
            at,
            erc_version: v,
            firmware: "4.38.23697",
            db_version: 174,
          });
          return {
            device: {
              device_id: "6f1c2b9e-3a1d-4c52-9a8e-2d1f0c7b5e44",
              serial: "N418xxxxxxxx",
              first_write: stamp("2026-10-31T09:12:00Z", "0.1.0"),
              last_write: stamp("2026-10-31T14:02:00Z", "0.1.0"),
            },
            books: [
              { content_id: contentId(1), path: "KEpub/Dune.kepub.epub" },
              { content_id: contentId(5), path: "KEpub/Pride.kepub.epub" },
            ],
            log: [
              {
                at: "2026-10-31T14:02:00Z",
                erc_version: "0.1.0",
                op: "collection",
                content_ids: [contentId(1)],
                detail: "add to “Favourites”",
                backup: "KoboReader-20261031-140200.000.sqlite",
              },
              {
                at: "2026-10-31T13:40:00Z",
                erc_version: "0.1.0",
                op: "replace",
                content_ids: [contentId(5)],
                detail: "Pride and Prejudice",
                error: "the Kobo database is busy",
              },
              {
                at: "2026-10-31T09:12:00Z",
                erc_version: "0.1.0",
                op: "send",
                content_ids: [contentId(1)],
              },
            ],
            problems: [],
            changed: [contentId(5)],
            missing: [],
          };
        }
        case "update_books": {
          const ids = a.ids as number[];
          for (const [i, id] of ids.entries()) {
            const base = { index: i + 1, total: ids.length, file: String(id) };
            await emit("send-progress", { stage: "copying", ...base });
            await sleep(300);
            await emit("send-progress", {
              stage: "done",
              ...base,
              device_path: "KEpub/x.kepub.epub",
            });
            onDevice.set(id, new Date().toISOString());
          }
          return ids.map((id) => ({
            source: String(id),
            book_id: id,
            result: { Ok: {} },
          }));
        }
        case "library_update": {
          const edit = a.edit as MetadataEdit;
          books = books.map((b) =>
            b.id === a.id
              ? { ...b, ...edit, modified_at: new Date().toISOString() }
              : b,
          );
          return books.find((b) => b.id === a.id);
        }
        case "library_remove":
          books = books.filter((b) => !(a.ids as number[]).includes(b.id));
          return null;
        case "plugin:dialog|open":
          return [
            "/Users/you/Downloads/my_scan_01.epub",
            "/Users/you/Downloads/no_author_notes.epub",
            "/Users/you/Downloads/The Long Way Home.epub",
            "/Users/you/Downloads/broken.epub",
          ];
        case "library_import":
          return (a.paths as string[]).map((path) => {
            const name = path.split("/").pop() ?? path;
            if (name.startsWith("broken"))
              return {
                path,
                outcome: { Err: "not a valid EPUB: no container.xml" },
              };
            const id = Math.max(...books.map((b) => b.id)) + 1;
            const book = {
              ...books[0],
              id,
              title: name.replace(/\.epub$/, ""),
              authors: name.startsWith("no_author") ? [] : ["A New Author"],
              series: null,
              series_index: null,
              tags: [],
              cover: null,
              has_cover: false,
              original_name: name,
            };
            books = [...books, book];
            return {
              path,
              outcome: {
                Ok: { kind: "added", book, possible_duplicate_of: null },
              },
            };
          });
        case "library_review": {
          const ids = a.ids as number[];
          const findings = ids.flatMap((id) => {
            const b = books.find((x) => x.id === id);
            const found = (issue: string) => ({
              issue,
              book_id: id,
              detail: "",
            });
            // Imports from calibre aren't really added here.
            if (!b) return [found("no_genre")];
            return [
              ...(b.tags.length ? [] : [found("no_genre")]),
              ...(b.authors.length ? [] : [found("no_author")]),
              ...(b.title.includes("_") ? [found("title_from_file")] : []),
              ...(b.has_cover ? [] : [found("no_cover")]),
            ];
          });
          return { findings, duplicates: [], bulk: ids.length > 25 };
        }
        case "send_books": {
          const ids = a.ids as number[];
          for (const [i, id] of ids.entries()) {
            const base = { index: i + 1, total: ids.length, file: String(id) };
            for (const stage of [
              "preparing",
              "converting",
              "copying",
            ] as const) {
              await emit("send-progress", { stage, ...base });
              await sleep(250);
            }
            await emit("send-progress", {
              stage: "done",
              ...base,
              device_path: "KEpub/x.kepub.epub",
            });
            onDevice.set(id, new Date().toISOString());
          }
          return ids.map((id) => ({
            source: String(id),
            book_id: id,
            result: { Ok: {} },
          }));
        }
        case "genres_plan":
          return {
            tags: [
              {
                from: "Sci-Fi",
                to: "Science Fiction",
                book_ids: [1, 2, 3],
                exact: true,
              },
              {
                from: "Non-Fiction",
                to: "Nonfiction",
                book_ids: [4],
                exact: true,
              },
              {
                from: "Space Opera",
                to: "Science Fiction",
                book_ids: [2],
                exact: false,
              },
            ],
            collections: a.mount
              ? [
                  {
                    from: "Sci-Fi",
                    to: "Science Fiction",
                    books: 21,
                    merge: false,
                  },
                  {
                    from: "SciFi",
                    to: "Science Fiction",
                    books: 2,
                    merge: true,
                  },
                ]
              : [],
          };
        case "genres_apply": {
          const choice = a.choice as {
            tags: unknown[];
            collections: unknown[];
          };
          await sleep(400);
          return {
            books: choice.tags.length * 2,
            collections: choice.collections.length,
          };
        }
        case "reading_stats": {
          const b = (
            title: string,
            author: string,
            extra: Record<string, unknown>,
          ) => ({
            content_id: title,
            title,
            author,
            percent: 0,
            time_secs: null,
            left_secs: null,
            last_read: null,
            finished_at: null,
            times_finished: 0,
            on_device: true,
            ...extra,
          });
          const finished = [
            ["We Are Legion (We Are Bob)", "Dennis E. Taylor", "2026-07-13"],
            ["Dungeon Crawler Carl", "Matt Dinniman", "2026-04-11"],
            ["A Mage of No Renown", "Kel Kade", "2026-03-16"],
            ["Red Rising", "Pierce Brown", "2024-10-10"],
            ["The Will of the Many", "James Islington", "2024-10-08"],
            ["The Hobbit", "J. R. R. Tolkien", "2024-01-05"],
            ...Array.from({ length: 14 }, (_, i) => [
              `Manga Volume ${i + 1}`,
              "Someone",
              `2023-12-${String(10 + i).padStart(2, "0")}`,
            ]),
            ["The Name of the Wind", "Rothfuss, Patrick", "2023-03-07"],
          ].map(([title, author, at], i) =>
            b(title, author, {
              percent: 100,
              finished_at: `${at}T20:00:00Z`,
              times_finished: i === 0 ? 2 : 1,
              on_device: i > 2 && i < 5,
            }),
          );
          const reading = [
            b("Iron Gold", "Pierce Brown", {
              percent: 7,
              time_secs: 14_400,
              left_secs: 103_000,
              last_read: "2026-10-30T20:00:00Z",
            }),
            b("I, Robot", "Isaac Asimov", {
              percent: 38,
              time_secs: 10_400,
              left_secs: 9_800,
              last_read: "2026-09-06T00:12:45Z",
            }),
            b("The Time Machine", "H. G. Wells", {
              percent: 20,
              time_secs: 2_100,
              left_secs: 7_900,
              last_read: "2026-06-19T21:31:50Z",
            }),
          ];
          return {
            device_serial: "N000",
            taken_at: "2026-11-04T22:00:00Z",
            stats: {
              device_model: "Kobo Libra 2",
              total_secs: 58_680,
              books_on_device: 90,
              reading,
              finished,
              most_time: [...reading].sort(
                (x, y) => (y.time_secs ?? 0) - (x.time_secs ?? 0),
              ),
            },
          };
        }
        case "calibre_default_library":
          return "/Users/you/Calibre Library";
        case "calibre_preview":
          return {
            total: 7,
            new: 4,
            already: 1,
            no_epub: [
              ["PDF", 1],
              ["AZW3, MOBI", 1],
            ],
          };
        case "calibre_import": {
          const titles = [
            "Hyperion",
            "The Fall of Hyperion",
            "Pandora's Star",
            "The Shadow of the Gods",
            "Dune",
            "A Scanned Book",
            "Kindle Thing",
          ];
          for (const [i, title] of titles.entries()) {
            await emit("import-progress", {
              index: i + 1,
              total: titles.length,
              title,
            });
            await sleep(200);
          }
          return titles.map((title, i) => ({
            calibre_id: i + 1,
            title,
            result:
              i < 4
                ? { Ok: { kind: "added", book_id: 100 + i } }
                : i === 4
                  ? { Ok: { kind: "matched", book_id: 1 } }
                  : {
                      Ok: {
                        kind: "skipped",
                        reason:
                          i === 5 ? "no EPUB (PDF)" : "no EPUB (AZW3, MOBI)",
                      },
                    },
          }));
        }
        case "import_from_device": {
          const wanted = a.contentIds as string[] | null;
          const all = [
            ...books
              .filter((b) => onDevice.has(b.id))
              .map((b) => ({ id: contentId(b.id), title: b.title })),
            ...storeBooks,
          ].filter((b) => !wanted || wanted.includes(b.id));
          const outcomes = [];
          for (const [i, b] of all.entries()) {
            await emit("import-progress", {
              index: i + 1,
              total: all.length,
              title: b.title,
            });
            await sleep(250);
            const sideload = storeBooks.find((s) => s.id === b.id);
            if (!sideload) {
              outcomes.push({
                content_id: b.id,
                title: b.title,
                result: {
                  Ok: { kind: "skipped", reason: "already in your library" },
                },
              });
              continue;
            }
            const id = Math.max(...books.map((x) => x.id)) + 1;
            const now = new Date().toISOString();
            books = [
              ...books,
              {
                ...books[0],
                id,
                title: sideload.title,
                authors: [sideload.author ?? "Unknown"],
                series: sideload.series,
                series_index: sideload.series_number,
                tags: [],
                original_name: `${sideload.title}.epub`,
                has_cover: false,
                cover: null,
                added_at: now,
                modified_at: now,
              },
            ];
            onDevice.set(id, now);
            storeBooks = storeBooks.filter((s) => s.id !== b.id);
            outcomes.push({
              content_id: b.id,
              title: b.title,
              result: { Ok: { kind: "added", book_id: id } },
            });
          }
          return outcomes;
        }
        case "library_check": {
          for (const [i] of books.entries()) {
            await emit("check-progress", {
              index: i + 1,
              total: books.length,
              title: "",
            });
            await sleep(40);
          }
          return [
            { check: "small_cover", book_ids: [2], detail: "521×850" },
            { check: "small_cover", book_ids: [4], detail: "655×1000" },
            { check: "no_cover", book_ids: [books.at(-2)!.id], detail: "" },
            {
              check: "broken_links",
              book_ids: [3],
              detail: "xhtml/titlepage.xhtml, xhtml/contents.xhtml",
            },
            {
              check: "series_gap",
              book_ids: [6, 7],
              detail: "Gentleman Bastard: #3 missing",
            },
            {
              check: "embedded_fonts",
              book_ids: [1],
              detail: "OEBPS/Fonts/Serif.otf",
            },
            {
              check: "clutter",
              book_ids: [5],
              detail: "__MACOSX/OEBPS/._c1.xhtml",
            },
          ];
        }
        case "annotations_list":
          return a.id === 3
            ? [
                {
                  id: "a1",
                  device_serial: "N000000000001",
                  kind: "highlight",
                  text: "I must not fear. Fear is the mind-killer.",
                  note: null,
                  chapter: "Book One: Dune",
                  position: 3.4,
                  created: "2026-10-20T20:00:00Z",
                  modified: null,
                  on_device: true,
                },
                {
                  id: "a2",
                  device_serial: "N000000000001",
                  kind: "note",
                  text: "The spice must flow.",
                  note: "Compare with the film.",
                  chapter: "Book One: Dune",
                  position: 3.9,
                  created: "2026-10-21T20:00:00Z",
                  modified: null,
                  on_device: true,
                },
                {
                  id: "a3",
                  device_serial: "N000000000001",
                  kind: "highlight",
                  text: "He who controls the spice controls the universe.",
                  note: null,
                  chapter: "Book Two: Muad'Dib",
                  position: 9.2,
                  created: "2026-10-22T20:00:00Z",
                  modified: null,
                  on_device: false,
                },
                {
                  id: "a4",
                  device_serial: "N000000000001",
                  kind: "bookmark",
                  text: null,
                  note: null,
                  chapter: "Book Two: Muad'Dib",
                  position: 10.1,
                  created: null,
                  modified: null,
                  on_device: true,
                },
              ]
            : [];
        case "annotations_export":
          return 4;
        case "device_report":
          await sleep(300);
          return null;
        case "plugin:dialog|save":
          return "/Users/you/Documents/Dune highlights.md";
        case "kobo_health":
          await sleep(300);
          return {
            missing_files: health.missing
              ? [
                  {
                    path: "KEpub/Old Book.kepub.epub",
                    content_id: "file:///mnt/onboard/KEpub/Old Book.kepub.epub",
                    title: "Old Book",
                  },
                ]
              : [],
            not_imported: [
              {
                path: "KEpub/Sent Today.kepub.epub",
                content_id: "file:///mnt/onboard/KEpub/Sent Today.kepub.epub",
                title: "Sent Today.kepub.epub",
              },
            ],
            duplicate_collections: health.duplicates
              ? [{ name: "Favourites", copies: 2 }]
              : [],
            orphaned_images: health.images,
            orphaned_image_bytes: health.images * 41_000,
          };
        case "clean_images": {
          const n = health.images;
          health.images = 0;
          return [n, n * 41_000];
        }
        case "merge_collections":
          health.duplicates = false;
          return 1;
        case "set_reading_status": {
          const ids = a.contentIds as string[];
          const status = a.status as DeviceBook["read_status"];
          storeBooks = storeBooks.map((b) =>
            ids.includes(b.id)
              ? {
                  ...b,
                  read_status: status,
                  percent_read:
                    status === "finished"
                      ? 100
                      : a.resetPosition
                        ? 0
                        : b.percent_read,
                }
              : b,
          );
          return ids.length;
        }
        case "enhance_propose": {
          const ids = a.ids as number[];
          const out = [];
          for (const [i, id] of ids.entries()) {
            const b = books.find((x) => x.id === id);
            if (!b) continue;
            await emit("enhance-progress", {
              index: i + 1,
              total: ids.length,
              title: b.title,
            });
            await sleep(120);
            const genres = b.tags.length
              ? []
              : [
                  {
                    name: "Nonfiction",
                    sources: ["Kobo folder"],
                    confident: true,
                  },
                  {
                    name: "Travel & Exploration",
                    sources: ["Open Library"],
                    confident: true,
                  },
                  {
                    name: "History",
                    sources: ["Apple Books"],
                    confident: false,
                  },
                ];
            out.push({
              book_id: id,
              series: b.series
                ? null
                : id % 3 === 0
                  ? {
                      name: "Dover Thrift Editions",
                      index: null,
                      source: "Wikidata",
                      confident: false,
                    }
                  : {
                      name: `${b.title.split(" ")[0]} Saga`,
                      index: "1",
                      source: "Wikidata",
                      confident: true,
                    },
              genres,
              problems: id === ids[0] ? ["Google Books: not set up"] : [],
            });
          }
          return out;
        }
        case "enhance_apply": {
          const accepted = a.accepted as {
            book_id: number;
            series: [string, string | null] | null;
            genres: string[];
          }[];
          const now = new Date().toISOString();
          const updated = [];
          for (const acc of accepted) {
            const b = books.find((x) => x.id === acc.book_id)!;
            const next = {
              ...b,
              series: acc.series ? acc.series[0] : b.series,
              series_index: acc.series ? acc.series[1] : b.series_index,
              tags: [...b.tags, ...acc.genres],
              modified_at: now,
            };
            books = books.map((x) => (x.id === b.id ? next : x));
            updated.push(next);
          }
          return updated;
        }
        case "setting_get":
          return settings.get(a.key as string) ?? null;
        case "setting_set":
          if (a.value) settings.set(a.key as string, a.value as string);
          else settings.delete(a.key as string);
          return null;
        case "library_duplicates": {
          const key = (b: Book) => b.title.split(":")[0].toLowerCase();
          const byKey = new Map<string, Book[]>();
          for (const b of books)
            byKey.set(key(b), [...(byKey.get(key(b)) ?? []), b]);
          return [...byKey.values()]
            .filter((g) => g.length > 1)
            .map((g) => ({
              key: g.map((b) => b.id).join("\n"),
              reasons: ["same_title_and_author"],
              members: g.map((b, i) => ({
                book_id: b.id,
                remove: i > 0 && !onDevice.has(b.id),
              })),
            }))
            .filter((g) => !dismissed.has(g.key));
        }
        case "device_duplicates":
          return [];
        case "dismiss_duplicates":
          dismissed.add(a.key as string);
          return null;
        case "eject_device":
          return null;
        case "cover_colors":
          return (a.paths as string[]).map((path) => {
            const hex = /fill="#([0-9a-f]{6})"/.exec(
              decodeURIComponent(path),
            )?.[1];
            if (!hex) return null;
            const n = parseInt(hex, 16);
            return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
          });
        case "cover_search":
          await sleep(600);
          return SAMPLE.slice(0, 6).map(([title, author], i) => {
            const url = coverSvg(title, author, i + 2);
            return {
              source: i % 2 ? "Open Library" : "Apple Books",
              url,
              preview_url: url,
            };
          });
        case "cover_set_url":
          books = books.map((b) =>
            b.id === a.id
              ? {
                  ...b,
                  cover: a.url as string,
                  has_cover: true,
                  custom_cover: true,
                  modified_at: new Date().toISOString(),
                }
              : b,
          );
          return books.find((b) => b.id === a.id);
        case "cover_upscale_available":
          return true;
        case "cover_upscale_tiles":
          return (a.ids as number[]).length * 12;
        case "cover_info": {
          // About what the real round trip takes, so the UI has to cope.
          await sleep(12);
          const book = books.find((b) => b.id === a.id);
          if (!book?.has_cover) return null;
          const height = [791, 1420, 2200][(a.id as number) % 3];
          return {
            width: Math.round(height * 0.66),
            height: book.cover_upscaled ? 1680 : height,
            quality: book.cover_upscaled
              ? "excellent"
              : height < 1200
                ? "low"
                : height < 1680
                  ? "decent"
                  : "excellent",
          };
        }
        case "cover_upscale_preview": {
          const book = books.find((b) => b.id === a.id);
          upscaleCancelled = false;
          for (let done = 1; done <= 12; done++) {
            await sleep(600);
            if (upscaleCancelled) throw "cancelled";
            await emit("upscale-progress", { id: a.id, done, total: 12 });
          }
          return {
            id: a.id,
            run: 1,
            before: book?.cover,
            after: book?.cover,
            before_size: [420, 630],
            after_size: [1120, 1680],
          };
        }
        case "cover_upscale_apply":
          books = books.map((b) =>
            b.id === a.id
              ? {
                  ...b,
                  custom_cover: true,
                  cover_upscaled: true,
                  modified_at: new Date().toISOString(),
                }
              : b,
          );
          return books.find((b) => b.id === a.id);
        case "cover_upscale_cancel":
          upscaleCancelled = true;
          return null;
        case "cover_reset":
          books = books.map((b) =>
            b.id === a.id
              ? { ...b, custom_cover: false, cover_upscaled: false }
              : b,
          );
          return books.find((b) => b.id === a.id);
        case "plugin:dialog|message": {
          // ask() resolves true when the returned label matches its OK label.
          const buttons = a.buttons as
            { OkCancelCustom?: [string, string] } | string | undefined;
          const ok =
            typeof buttons === "object" && buttons.OkCancelCustom
              ? buttons.OkCancelCustom[0]
              : "Yes";
          return window.confirm(String(a.message ?? "Are you sure?"))
            ? ok
            : "No";
        }
        default:
          // Plugin and event-system calls the UI doesn't depend on.
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
