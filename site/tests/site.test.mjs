// Builds the site against a fixed set of releases and checks what a visitor
// gets: the newest stable disk image behind the download button, and every
// published version on the download page.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { before, test } from 'node:test';

const site = new URL('..', import.meta.url).pathname;
let out;

before(() => {
  out = mkdtempSync(join(tmpdir(), 'parallax-site-'));
  execFileSync('pnpm', ['exec', 'astro', 'build', '--outDir', out], {
    cwd: site,
    env: {
      ...process.env,
      PARALLAX_RELEASES_FIXTURE: join(site, 'tests/fixtures/releases.json'),
      PARALLAX_REQUIRE_RELEASES: '1',
    },
    stdio: 'pipe',
  });
});

const page = (path) => readFileSync(join(out, path), 'utf8');
const downloads = (html) =>
  [
    ...html.matchAll(
      /<a[^>]*data-download[^>]*href="([^"]+)"|<a[^>]*href="([^"]+)"[^>]*data-download/g,
    ),
  ].map((m) => m[1] ?? m[2]);

test('the download button fetches the newest stable release, not a beta', () => {
  assert.deepEqual(downloads(page('index.html')), [
    'https://example.test/Parallax_0.1.1_universal.dmg',
  ]);
  assert.match(page('index.html'), /Version 0\.1\.1 · 48\.2 MB/);
  assert.deepEqual(downloads(page('download/index.html')), [
    'https://example.test/Parallax_0.1.1_universal.dmg',
  ]);
});

test('the download page lists every published version, newest first', () => {
  const html = page('download/index.html');
  const order = ['v0.2.0-beta.1', 'v0.1.1', 'v0.1.0'].map((tag) => html.indexOf(`id="${tag}"`));
  assert.ok(
    order.every((i) => i > 0),
    'every release has an entry',
  );
  assert.deepEqual(
    [...order].sort((a, b) => a - b),
    order,
  );
  assert.ok(!html.includes('id="v0.2.0"'), 'drafts stay hidden');
  assert.match(html, /cli-arm\.tar\.gz/);
  assert.match(html, /<strong>Check for Updates…<\/strong>/);
});

test('the favicon is the app icon', () => {
  assert.equal(
    readFileSync(join(site, 'public/favicon.svg'), 'utf8'),
    readFileSync(join(site, '../app/public/parallax.svg'), 'utf8'),
  );
});

test('the app snapshot is static and complete', () => {
  const html = page('app-snapshot/index.html');
  assert.ok(!/<script/i.test(html), 'no scripts run in the hero');
  const files = [...html.matchAll(/(?:src="|url\(["']?)((?:covers|fonts)\/[^"')]+)/g)].map(
    (m) => m[1],
  );
  assert.ok(
    files.some((f) => f.startsWith('covers/')),
    'covers are files',
  );
  assert.ok(
    files.some((f) => f.startsWith('fonts/')),
    'fonts are files',
  );
  for (const file of files) {
    assert.ok(existsSync(join(out, 'app-snapshot', file)), `${file} is published`);
  }
  assert.match(page('index.html'), /data-src="\/app-snapshot\/index\.html"/);
});
