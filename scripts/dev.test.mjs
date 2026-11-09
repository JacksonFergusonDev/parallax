import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("./dev.sh", import.meta.url));

async function fixture(t, { mode = "conflict", released = false, count = 1 } = {}) {
  const dir = await mkdtemp(join(tmpdir(), "parallax-dev-test-"));
  const children = Array.from({ length: count }, () =>
    spawn(process.execPath, ["-e", "setInterval(() => {}, 1000)"], {
      stdio: "ignore",
    }),
  );
  t.after(async () => {
    for (const child of children) child.kill();
    await rm(dir, { recursive: true, force: true });
  });
  const calls = join(dir, "calls");
  await writeFile(calls, "");
  await writeFile(
    join(dir, "pnpm"),
    `#!/bin/bash
[[ "$*" == 'tauri dev' ]] || exit 99
if [[ -s "$CALLS" ]]; then
    echo retry
    exit 0
fi
echo called >> "$CALLS"
case "$MODE" in
    success) exit 0 ;;
    unrelated) echo 'Rust compilation failed' >&2; exit 17 ;;
    *) echo 'Error: Port 1420 is already in use' >&2; exit 23 ;;
esac
`,
    { mode: 0o755 },
  );
  await writeFile(
    join(dir, "lsof"),
    `#!/bin/bash
[[ "$*" == '-nP -t -iTCP:1420 -sTCP:LISTEN' ]] || exit 99
if [[ "$RELEASED" == 1 && -e "$CALLS.lsof" ]]; then exit 1; fi
touch "$CALLS.lsof"
found=1
for pid in $LISTENER_PIDS; do
    if kill -0 "$pid" 2>/dev/null; then echo "$pid"; found=0; fi
done
exit "$found"
`,
    { mode: 0o755 },
  );
  return {
    children,
    async run(answer = "") {
      const child = spawn("bash", [script], {
        env: {
          ...process.env,
          PATH: `${dir}${delimiter}${process.env.PATH}`,
          CALLS: calls,
          MODE: mode,
          LISTENER_PIDS: children.map((child) => child.pid).join(" "),
          RELEASED: released ? "1" : "0",
        },
        stdio: ["pipe", "pipe", "pipe"],
      });
      t.after(() => child.kill());
      let output = "";
      child.stdout.on("data", (data) => (output += data));
      child.stderr.on("data", (data) => (output += data));
      child.stdin.on("error", () => {});
      child.stdin.end(answer);
      const code = await new Promise((resolve, reject) => {
        child.once("error", reject);
        child.once("close", resolve);
      });
      return { code, output, calls: await readFile(calls, "utf8") };
    },
  };
}

for (const [name, answer] of [
  ["no", "n\n"],
  ["empty answer", "\n"],
  ["EOF", ""],
  ["unrecognised answer", "maybe\n"],
]) {
  test(`declining with ${name} preserves listeners and failure status`, async (t) => {
    const f = await fixture(t);
    const result = await f.run(answer);
    assert.equal(result.code, 23);
    assert.match(result.output, /\[y\/N\]/);
    assert.equal(f.children[0].exitCode, null);
    assert.equal(f.children[0].signalCode, null);
    assert.doesNotMatch(result.output, /Retrying/);
  });
}

for (const answer of ["y", "Y", "yes"]) {
  test(`${answer} terminates all approved listeners and retries Tauri`, async (t) => {
    const f = await fixture(t, { count: 2 });
    const result = await f.run(`${answer}\n`);
    assert.equal(result.code, 0, result.output);
    assert.match(result.output, /Retrying app startup/);
    assert.match(result.output, /retry/);
    for (const child of f.children) assert.equal(child.signalCode, "SIGTERM");
  });
}

test("unrelated failures retain their exit status without prompting", async (t) => {
  const f = await fixture(t, { mode: "unrelated" });
  const result = await f.run("y\n");
  assert.equal(result.code, 17);
  assert.doesNotMatch(result.output, /\[y\/N\]/);
  assert.equal(f.children[0].signalCode, null);
});

test("successful startup exits without prompting", async (t) => {
  const f = await fixture(t, { mode: "success" });
  const result = await f.run();
  assert.equal(result.code, 0);
  assert.doesNotMatch(result.output, /\[y\/N\]/);
});

test("a process that released the port before approval is not killed", async (t) => {
  const f = await fixture(t, { released: true });
  const result = await f.run("y\n");
  assert.equal(result.code, 0, result.output);
  assert.match(result.output, /Retrying/);
  assert.equal(f.children[0].signalCode, null);
});

test("a conflict with no remaining listener exits without prompting", async (t) => {
  const f = await fixture(t, { count: 0 });
  const result = await f.run("y\n");
  assert.equal(result.code, 23);
  assert.match(result.output, /No listener found/);
  assert.doesNotMatch(result.output, /\[y\/N\]/);
});
