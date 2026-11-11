// Saves the app's library window, as the browser mock draws it, into
// public/app-snapshot/ for the landing page's hero. The result is the real
// markup and CSS frozen as a static page: no scripts, so it can't run or
// change, but its text and covers stay sharp at any size. Fonts and covers
// are written beside it; other CSS assets are inlined. The library is the
// public-domain showcase in scripts/showcase/, not the mock's sample.
//
// Run `just site-snapshot` after a visible change to the library window, and
// commit what it writes. It starts the app's Vite dev server (the mock only
// runs in development) and drives it with Playwright's Chromium.
import { spawn } from 'node:child_process';
import { mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { chromium } from 'playwright';

const SITE = new URL('..', import.meta.url).pathname;
const APP = join(SITE, '../app');
const OUT = join(SITE, 'public/app-snapshot');
const PORT = 1437;
// The app's default window, less the 28px title bar the site draws.
const VIEWPORT = { width: 1280, height: 772 };
const BOOK = process.argv[2] ?? 'Dracula';
const SHOWCASE = join(SITE, 'scripts/showcase');

async function showcase() {
  const books = JSON.parse(await readFile(join(SHOWCASE, 'books.json'), 'utf8'));
  for (const book of books) {
    const jpeg = await readFile(join(SHOWCASE, 'covers', book.cover));
    book.cover = `data:image/jpeg;base64,${jpeg.toString('base64')}`;
  }
  return books;
}

function startServer() {
  const server = spawn('pnpm', ['exec', 'vite', '--port', String(PORT), '--strictPort'], {
    cwd: APP,
    stdio: ['ignore', 'pipe', 'inherit'],
  });
  return new Promise((resolve, reject) => {
    server.stdout.on('data', (chunk) => {
      if (String(chunk).includes(`:${PORT}`)) resolve(server);
    });
    server.on('exit', (code) => reject(new Error(`vite exited with ${code}`)));
  });
}

// Runs in the page: the rendered window as one static document.
async function serialize() {
  const toDataUrl = async (url) => {
    const blob = await (await fetch(url)).blob();
    return new Promise((resolve) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result);
      reader.readAsDataURL(blob);
    });
  };
  const urls = (text) => [...text.matchAll(/url\((['"]?)([^'")]+)\1\)/g)].map((m) => m[2]);

  // Only the faces this window loaded, and of Fraunces only its Latin subset.
  const fonts = [];
  const unquote = (value) => value.replace(/["']/g, '').trim();
  const loaded = new Set(
    [...document.fonts]
      .filter((face) => face.status === 'loaded')
      .map((face) => `${unquote(face.family)}|${face.weight}|${face.style}`),
  );
  const used = (style) =>
    loaded.has(
      `${unquote(style.getPropertyValue('font-family'))}|${style.getPropertyValue('font-weight') || '400'}|${style.getPropertyValue('font-style') || 'normal'}`,
    );
  const latin = (range) => !range || /U\+0(000)?-0?0?FF\b/i.test(range);
  let css = '';
  for (const sheet of document.styleSheets) {
    let rules;
    try {
      rules = sheet.cssRules;
    } catch {
      continue;
    }
    for (const rule of rules) {
      let text = rule.cssText;
      if (rule instanceof CSSFontFaceRule) {
        if (!used(rule.style) || !latin(rule.style.getPropertyValue('unicode-range'))) continue;
        for (const url of urls(text)) {
          const name = `${fonts.length}-${url.split('/').pop().split('?')[0]}`;
          const data = (await toDataUrl(url)).split(',')[1];
          fonts.push({ name, data });
          text = text.replace(url, `fonts/${name}`);
        }
      } else {
        for (const url of urls(text)) {
          if (!url.startsWith('data:') && !url.startsWith('#')) {
            text = text.replace(url, await toDataUrl(url));
          }
        }
      }
      css += `${text}\n`;
    }
  }

  // Form state lives in properties, which outerHTML leaves out.
  for (const input of document.querySelectorAll('input')) {
    if (input.type === 'checkbox' || input.type === 'radio') {
      input.toggleAttribute('checked', input.checked);
    } else {
      input.setAttribute('value', input.value);
    }
  }
  for (const option of document.querySelectorAll('option')) {
    option.toggleAttribute('selected', option.selected);
  }
  // Covers become files, once each, however many times the window shows them.
  const images = [];
  const named = new Map();
  for (const img of document.querySelectorAll('img')) {
    if (!img.src) continue;
    const data = img.src.startsWith('data:') ? img.src : await toDataUrl(img.src);
    const [, type, payload] = /^data:image\/([a-z+]+);base64,(.*)$/.exec(data) ?? [];
    if (!payload) {
      img.src = data;
      continue;
    }
    if (!named.has(payload)) {
      const name = `covers/${named.size}.${type === 'jpeg' ? 'jpg' : type.replace('+xml', '')}`;
      named.set(payload, name);
      images.push({ name, data: payload });
    }
    img.setAttribute('src', named.get(payload));
    img.setAttribute('loading', 'lazy');
    img.removeAttribute('srcset');
  }
  for (const canvas of document.querySelectorAll('canvas')) {
    const img = document.createElement('img');
    img.src = canvas.toDataURL();
    img.className = canvas.className;
    img.style.cssText = canvas.style.cssText;
    canvas.replaceWith(img);
  }
  for (const node of document.querySelectorAll('script, link, noscript')) node.remove();

  const attrs = (el) =>
    [...el.attributes].map((a) => ` ${a.name}="${a.value.replaceAll('"', '&quot;')}"`).join('');
  const html = `<!doctype html>
<html${attrs(document.documentElement)}>
<head>
<meta charset="utf-8">
<meta name="robots" content="noindex">
<title>Parallax</title>
<style>${css}</style>
<style>html, body { overflow: hidden; } *, *::before, *::after { animation: none !important; transition: none !important; caret-color: transparent !important; }</style>
</head>
<body${attrs(document.body)} inert>${document.body.innerHTML}</body>
</html>
`;
  return { html, fonts, images };
}

const server = await startServer();
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: VIEWPORT, deviceScaleFactor: 2 });
  await page.addInitScript(
    (books) => {
      window.__PARALLAX_SHOWCASE__ = books;
    },
    await showcase(),
  );
  await page.emulateMedia({ colorScheme: 'dark', reducedMotion: 'reduce' });
  await page.goto(`http://localhost:${PORT}/?theme=dark`, { waitUntil: 'networkidle' });
  // Select a book, so the detail panel shows what Parallax knows about it.
  await page.locator('.card:visible', { hasText: BOOK }).first().click();
  await page.mouse.move(0, VIEWPORT.height - 1);
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(500);
  const { html, fonts, images } = await page.evaluate(serialize);

  await rm(OUT, { recursive: true, force: true });
  await mkdir(join(OUT, 'fonts'), { recursive: true });
  await mkdir(join(OUT, 'covers'), { recursive: true });
  for (const image of images) {
    await writeFile(join(OUT, image.name), Buffer.from(image.data, 'base64'));
  }
  await writeFile(join(OUT, 'index.html'), html);
  for (const font of fonts) {
    await writeFile(join(OUT, 'fonts', font.name), Buffer.from(font.data, 'base64'));
  }
  console.log(
    `Wrote the snapshot, ${fonts.length} fonts and ${images.length} images to public/app-snapshot/`,
  );
} finally {
  await browser.close();
  server.kill();
}
