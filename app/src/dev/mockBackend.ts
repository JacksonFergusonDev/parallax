// Browser-only stand-in for the Rust backend, so the UI can be developed and
// screenshotted with `pnpm dev` in a normal browser. Loaded only in dev builds
// when not running inside Tauri; never part of the app bundle.
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Book, DeviceInfo, MetadataEdit } from "../api";

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
  description: null,
  identifiers: [],
  original_name: `${title}.epub`,
  file_size: 1_000_000,
  has_cover: i < SAMPLE.length - 1,
  added_at: "2026-10-31T00:00:00Z",
  modified_at: "2026-10-31T00:00:00Z",
  cover: i < SAMPLE.length - 1 ? coverSvg(title, author, i) : null,
}));

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
};
const onDevice = new Set([3]);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

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
          return [...onDevice];
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
        case "library_import":
          return [];
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
            onDevice.add(id);
          }
          return ids.map((id) => ({
            source: String(id),
            book_id: id,
            result: { Ok: {} },
          }));
        }
        case "eject_device":
          return null;
        default:
          // Plugin and event-system calls the UI doesn't depend on.
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
