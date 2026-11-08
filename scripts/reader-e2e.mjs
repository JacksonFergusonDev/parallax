// Runs the reader in a real build of the app against books in
// tests/fixtures/reader, in an empty library under a temporary HOME. Run it
// with `just test-reader`. Windows open on screen while it runs.
//
// The hostile book's scripts save reader settings, which a reader window may
// do, so any script that runs leaves a mark. A control run with the reader's
// CSP loosened must leave that mark, or the test can't see anything.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DatabaseSync } from "node:sqlite";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const fixtures = join(root, "tests/fixtures/reader");
const bin = join(root, "target/e2e/debug");
/** The address hostile.epub sends requests to; see its make.py. */
const BEACON_PORT = 8931;
const RUN_TIMEOUT_MS = 120_000;
const view = "document.querySelector('foliate-view')";

function run(command, args, { env = {}, timeout = 600_000 } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: root,
      env: { ...process.env, ...env },
      stdio: ["ignore", "pipe", "pipe"],
    });
    let output = "";
    child.stdout.on("data", (d) => (output += d));
    child.stderr.on("data", (d) => (output += d));
    const timer = setTimeout(() => child.kill(), timeout);
    child.on("error", reject);
    child.on("close", (code) => {
      clearTimeout(timer);
      if (code === 0) resolve(output);
      else reject(new Error(`${command} ${args.join(" ")} exited ${code}\n${output}`));
    });
  });
}

let home;
let requests = [];
let server;
const ids = {};

const libraryFile = () =>
  join(home, "Library/Application Support/dev.erc.companion/library/library.sqlite");

function query(sql, ...params) {
  const db = new DatabaseSync(libraryFile());
  try {
    return db.prepare(sql).all(...params);
  } finally {
    db.close();
  }
}

const readerSettings = () =>
  query("SELECT value FROM settings WHERE key = 'reader'")[0]?.value ?? null;
const position = (id) =>
  query("SELECT * FROM reader_positions WHERE book_id = ?", id)[0] ?? null;

/** Opens book `id` alone in a reader, runs `steps` in it, and waits for it to quit. */
function read(id, steps = [], env = {}) {
  return run(join(bin, "Parallax"), [], {
    timeout: RUN_TIMEOUT_MS,
    env: {
      HOME: home,
      ERC_E2E_OPEN: String(id),
      ERC_E2E_STEPS: steps.join("@@"),
      ERC_E2E_SECS: "4",
      ...env,
    },
  });
}

const everySection = [0, 1, 2, 3].map((i) => `${view}.goTo(${i})`);

/** Searches through the reader's own panel, then picks the first match. */
const searchFor = (text) => [
  `document.getElementById('find').click();` +
    `document.getElementById('query').value = ${JSON.stringify(text)};` +
    `document.querySelector('#search form').requestSubmit()`,
  `document.querySelector('#results button').click()`,
];

before(async () => {
  // READER_E2E_NO_BUILD=1 reuses the last build, when only the test changed.
  if (!process.env.READER_E2E_NO_BUILD) {
    await run("pnpm", ["--dir", "app", "build"]);
    await run("cargo", [
      "build",
      "-p", "erc-app",
      "-p", "erc-cli",
      "--features", "erc-app/e2e",
      "--target-dir", "target/e2e",
    ]);
  }
  home = await mkdtemp(join(tmpdir(), "parallax-reader-e2e-"));
  await run(join(bin, "erc"), [
    "library", "add",
    join(fixtures, "hostile.epub"),
    join(fixtures, "long.epub"),
  ], { env: { HOME: home } });
  const books = JSON.parse(
    await run(join(bin, "erc"), ["--json", "library", "ls"], { env: { HOME: home } }),
  );
  for (const book of books) ids[book.title] = book.id;
  assert.ok(ids.Hostile && ids.Long, "fixtures weren't added");

  server = createServer((request, response) => {
    requests.push(request.url);
    response.end();
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(BEACON_PORT, "127.0.0.1", resolve);
  });
});

after(async () => {
  server?.close();
  if (home) await rm(home, { recursive: true, force: true });
});

test("a hostile book runs no scripts and reaches no network", async () => {
  requests = [];
  // Search reads every section off screen as well as showing them, and lands
  // on the last one.
  await read(ids.Hostile, [...everySection.slice(0, 3), ...searchFor("Last")]);
  assert.deepEqual(requests, [], "the book reached the network");
  assert.equal(readerSettings(), null, "a script in the book ran");
  assert.equal(query("SELECT id FROM books").length, 2, "a book was removed");
  // It was really shown: walking to the last section saved a position there.
  assert.equal(position(ids.Hostile)?.href, "c3.xhtml");
});

test("control: with the CSP loosened, the same scripts are caught", async () => {
  requests = [];
  await read(ids.Hostile, everySection, { ERC_E2E_LOOSE: "1" });
  assert.match(readerSettings() ?? "", /"fontSize":200/, "scripts ran but weren't seen");
  assert.ok(requests.some((url) => url.startsWith("/exec/")), "no script reached the network");
  // The ACL holds even when the CSP doesn't.
  assert.equal(query("SELECT id FROM books").length, 2, "a book was removed");
});

test("reopening returns to the same place after the text reflows", async () => {
  const db = new DatabaseSync(libraryFile());
  db.prepare("INSERT OR REPLACE INTO settings (key, value) VALUES ('reader', ?)").run(
    JSON.stringify({ fontSize: 120, theme: "sepia" }),
  );
  db.close();

  await read(ids.Long, Array(5).fill(`${view}.next()`));
  const left = position(ids.Long);
  assert.ok(left && left.fraction > 0, "turning pages saved nothing");

  await new Promise((resolve) => setTimeout(resolve, 1100));
  await read(ids.Long);
  const reopened = position(ids.Long);
  assert.notEqual(reopened.updated_at, left.updated_at, "reopening didn't save");
  assert.equal(reopened.cfi, left.cfi);
});

test("searching for a line goes to it", async () => {
  await read(ids.Long, searchFor("Paragraph 4.7."));
  const found = position(ids.Long);
  assert.equal(found?.href, "c3.xhtml", "search didn't go to chapter 4");
  assert.match(found.quote ?? "", /Paragraph 4\.7\./);
});
