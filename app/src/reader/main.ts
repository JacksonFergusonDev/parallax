import { isTauri } from "@tauri-apps/api/core";
import { EPUB } from "foliate-js/epub.js";
import type { Location, View } from "foliate-js/view.js";
import "foliate-js/view.js";
import { bookStyles, paletteOf, shownTheme, stepFontSize } from "./appearance";
import {
  bookLoader,
  openedBook,
  savePosition,
  saveSettings,
  type OpenedBook,
  type Position,
  type Settings,
  type Theme,
} from "./backend";
import { contentsList, markCurrent } from "./contents";
import { keyAction, percent, wheelPager, within } from "./paging";
import "./reader.css";

const OPEN_TIMEOUT_MS = 15_000;
const TURN_TIMEOUT_MS = 8_000;
/** Enough of the page's text to recognise the place by. */
const QUOTE_LENGTH = 120;

const element = <T extends HTMLElement>(id: string) =>
  document.getElementById(id) as T;

function showMessage(text: string) {
  const message = element("message");
  message.textContent = text;
  message.hidden = false;
}

const nextFrame = () => new Promise((done) => requestAnimationFrame(done));

/** Goes back to where the book was left, by the best route still working. */
async function resume(view: View, position: Position | null) {
  try {
    await within(
      view.init({ lastLocation: position?.cfi, showTextStart: true }),
      OPEN_TIMEOUT_MS,
    );
  } catch {
    if (position) await view.goToFraction(position.fraction);
    else await view.goTo(0);
    return;
  }
  if (!position) return;
  // The page reflows once its fonts and the reader's text size apply, which
  // moves it off the saved place, so go there again once that has settled.
  const doc = view.renderer.getContents?.()[0]?.doc;
  await within<unknown>(doc?.fonts.ready ?? Promise.resolve(), 2_000).catch(
    () => {},
  );
  await nextFrame();
  await nextFrame();
  await within(view.goTo(position.cfi), TURN_TIMEOUT_MS).catch(console.error);
}

/** Sends the latest position, one at a time, skipping any overtaken. */
function positionSaver(sections: { id: string }[]) {
  let waiting: Position | null = null;
  let saving = false;
  const flush = async () => {
    saving = true;
    while (waiting) {
      const position = waiting;
      waiting = null;
      // A removed book can't be saved, and the reader has nothing to do about it.
      await savePosition(position).catch(console.error);
    }
    saving = false;
  };
  return ({ cfi, fraction, range, section }: Location) => {
    const href = sections[section?.current ?? 0]?.id;
    if (!cfi || !href) return;
    const quote = range?.toString().trim().slice(0, QUOTE_LENGTH) || null;
    waiting = { href, cfi, fraction, quote };
    if (!saving) void flush();
  };
}

function setUpAppearance(view: View, initial: Settings) {
  let settings = initial;
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
  const apply = () => {
    const theme = shownTheme(settings.theme, systemDark.matches);
    const { background, text, link } = paletteOf(theme);
    const { style } = document.documentElement;
    style.setProperty("--page", background);
    style.setProperty("--ink", text);
    style.setProperty("--link", link);
    style.setProperty("--accent", link);
    style.colorScheme = theme === "dark" ? "dark" : "light";
    document.documentElement.dataset.theme = theme;
    view.renderer.setStyles?.(bookStyles(settings, theme));
    element("size").textContent = `${settings.fontSize}%`;
    for (const button of document.querySelectorAll<HTMLElement>(
      "[data-theme]:not(html)",
    )) {
      const on = button.dataset.theme === settings.theme;
      button.setAttribute("aria-checked", String(on));
    }
  };
  const change = (next: Partial<Settings>) => {
    settings = { ...settings, ...next };
    apply();
    saveSettings(settings).catch(console.error);
  };
  const resize = (direction: 1 | -1) =>
    change({ fontSize: stepFontSize(settings.fontSize, direction) });

  systemDark.addEventListener("change", apply);
  element("smaller").addEventListener("click", () => resize(-1));
  element("larger").addEventListener("click", () => resize(1));
  for (const button of document.querySelectorAll<HTMLElement>(
    "[data-theme]:not(html)",
  )) {
    button.addEventListener("click", () =>
      change({ theme: button.dataset.theme as Theme }),
    );
  }
  apply();
  return { resize };
}

function setUpPanels() {
  const panels = [
    { button: element("contents"), panel: element("toc") },
    { button: element("appearance"), panel: element("settings") },
  ];
  const close = () => {
    for (const { button, panel } of panels) {
      panel.hidden = true;
      button.setAttribute("aria-expanded", "false");
    }
  };
  for (const { button, panel } of panels) {
    button.addEventListener("click", () => {
      const opening = panel.hidden;
      close();
      panel.hidden = !opening;
      button.setAttribute("aria-expanded", String(opening));
    });
  }
  return { close };
}

async function read(opened: OpenedBook) {
  document.title = opened.title;
  element("title").textContent = opened.title;

  const book = await new EPUB(bookLoader(opened.entries)).init();
  const view = document.createElement("foliate-view") as View;
  element("book").append(view);
  await view.open(book);
  view.renderer.setAttribute("flow", "paginated");
  view.renderer.setAttribute("max-inline-size", "640px");
  view.renderer.setAttribute("max-column-count", "2");

  const panels = setUpPanels();
  const { resize } = setUpAppearance(view, opened.settings);

  const turnWith = (turn: () => Promise<unknown>) => {
    within(turn(), TURN_TIMEOUT_MS).catch(console.error);
  };
  const next = () => turnWith(() => view.next());
  const previous = () => turnWith(() => view.prev());
  const onKey = (event: KeyboardEvent) => {
    const actions = {
      left: () => turnWith(() => view.goLeft()),
      right: () => turnWith(() => view.goRight()),
      next,
      previous,
      larger: () => resize(1),
      smaller: () => resize(-1),
      close: panels.close,
    };
    const action = keyAction(event);
    if (!action) return;
    event.preventDefault();
    actions[action]();
  };
  const onWheel = wheelPager((direction) =>
    direction > 0 ? next() : previous(),
  );

  window.addEventListener("keydown", onKey);
  window.addEventListener("wheel", onWheel, { passive: true });
  element("prev").addEventListener("click", () =>
    turnWith(() => view.goLeft()),
  );
  element("next").addEventListener("click", () =>
    turnWith(() => view.goRight()),
  );

  // Pages are in frames of their own, whose events don't reach this window.
  view.addEventListener("load", (event) => {
    const { doc } = (event as CustomEvent<{ doc: Document }>).detail;
    doc.addEventListener("keydown", onKey);
    doc.addEventListener("wheel", onWheel, { passive: true });
    doc.addEventListener("click", panels.close);
  });
  // No opening the web from a book. Cancelling stops the engine doing so.
  view.addEventListener("external-link", (event) => event.preventDefault());

  const toc = element("toc");
  toc.replaceChildren(
    contentsList(book.toc ?? [], (href) => {
      panels.close();
      turnWith(() => view.goTo(href));
    }),
  );

  await resume(view, opened.position);

  const remember = positionSaver(book.sections);
  const show = (location: Location) => {
    element("chapter").textContent = location.tocItem?.label ?? "";
    element("progress").textContent = percent(location.fraction);
    markCurrent(toc, location.tocItem?.href)?.scrollIntoView({
      block: "nearest",
    });
  };
  view.addEventListener("relocate", (event) => {
    const location = (event as CustomEvent<Location>).detail;
    show(location);
    remember(location);
  });
  if (view.lastLocation) show(view.lastLocation);
}

async function start() {
  if (import.meta.env.DEV && !isTauri()) {
    const { installMockReader } = await import("./mockReader");
    installMockReader();
  }
  try {
    await read(await openedBook());
  } catch (error) {
    console.error(error);
    showMessage(`This book can't be opened. ${error}`);
  }
}

start();
