import { isTauri } from "@tauri-apps/api/core";
import { EPUB, type Book, type TocItem } from "foliate-js/epub.js";
import { FootnoteHandler, type FootnoteRender } from "foliate-js/footnotes.js";
import { Overlayer } from "foliate-js/overlayer.js";
import type {
  DrawAnnotation,
  LinkEvent,
  Location,
  ShowAnnotation,
  View,
} from "foliate-js/view.js";
import "foliate-js/view.js";
import {
  bookStyles,
  layoutAttributes,
  paletteOf,
  shownTheme,
  stepFontSize,
} from "./appearance";
import {
  bookLoader,
  deleteHighlight,
  highlights as loadHighlights,
  openedBook,
  saveHighlight,
  savePosition,
  saveSettings,
  type Highlight,
  type HighlightColor,
  type OpenedBook,
  type Position,
  type Settings,
} from "./backend";
import { contentsList, markCurrent } from "./contents";
import {
  COLOR_VALUES,
  MAX_TEXT,
  asMarkdown,
  boxInWindow,
  highlightList,
  offsetIn,
  placeNear,
} from "./highlights";
import { keyAction, percent, wheelPager, within } from "./paging";
import { chapterMarks, whereabouts } from "./progress";
import { groupItem, searcher, statusText, stepMatch } from "./search";
import { Speaker, nextRate, type SpeechSource } from "./speech";
import "./reader.css";

const OPEN_TIMEOUT_MS = 15_000;
const TURN_TIMEOUT_MS = 8_000;
/** Enough of the page's text to recognise the place by. */
const QUOTE_LENGTH = 120;
/** The slider's steps; fine enough for a long book. */
const SLIDER_STEPS = 1000;

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

/** Text size, theme and layout, kept on the page and saved as they change. */
function setUpAppearance(view: View, initial: Settings) {
  let settings = initial;
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
  const choices = document.querySelectorAll<HTMLElement>("[data-setting]");
  const listeners: (() => void)[] = [];
  const theme = () => shownTheme(settings.theme, systemDark.matches);
  const styles = () => bookStyles(settings, theme());
  const apply = () => {
    const { background, text, link } = paletteOf(theme());
    const { style } = document.documentElement;
    style.setProperty("--page", background);
    style.setProperty("--ink", text);
    style.setProperty("--link", link);
    style.setProperty("--accent", link);
    style.colorScheme = theme() === "dark" ? "dark" : "light";
    document.documentElement.dataset.theme = theme();
    for (const [name, value] of Object.entries(layoutAttributes(settings))) {
      if (view.renderer.getAttribute(name) !== value)
        view.renderer.setAttribute(name, value);
    }
    view.renderer.setStyles?.(styles());
    element("size").textContent = `${settings.fontSize}%`;
    for (const choice of choices) {
      const key = choice.dataset.setting as keyof Settings;
      const on = String(settings[key]) === choice.dataset.value;
      choice.setAttribute("aria-checked", String(on));
    }
    for (const listener of listeners) listener();
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
  for (const choice of choices) {
    choice.addEventListener("click", () =>
      change({ [choice.dataset.setting!]: choice.dataset.value }),
    );
  }
  apply();
  return {
    resize,
    styles,
    scrolled: () => settings.flow === "scrolled",
    /** Calls `listener` whenever the appearance changes. */
    onChange: (listener: () => void) => listeners.push(listener),
  };
}

const PANELS = {
  contents: ["contents", "toc"],
  notes: ["notes-button", "notes"],
  search: ["find", "search"],
  appearance: ["appearance", "settings"],
} as const;
type PanelName = keyof typeof PANELS;

function setUpPanels(onOpen: (name: PanelName) => void) {
  const panels = Object.fromEntries(
    Object.entries(PANELS).map(([name, [button, panel]]) => [
      name,
      { button: element(button), panel: element(panel) },
    ]),
  ) as Record<PanelName, { button: HTMLElement; panel: HTMLElement }>;
  const close = () => {
    for (const { button, panel } of Object.values(panels)) {
      panel.hidden = true;
      button.setAttribute("aria-expanded", "false");
    }
  };
  const open = (name: PanelName) => {
    close();
    panels[name].panel.hidden = false;
    panels[name].button.setAttribute("aria-expanded", "true");
    onOpen(name);
  };
  for (const [name, { button, panel }] of Object.entries(panels)) {
    button.addEventListener("click", () => {
      if (panel.hidden) open(name as PanelName);
      else close();
    });
  }
  return { close, open };
}
type Panels = ReturnType<typeof setUpPanels>;

/** The search panel, and stepping through its matches with ⌘G. */
function setUpSearch(view: View, panels: Panels, go: (target: string) => void) {
  const query = element<HTMLInputElement>("query");
  const wholeWords = element<HTMLInputElement>("whole-words");
  const matchCase = element<HTMLInputElement>("match-case");
  const matchAccents = element<HTMLInputElement>("match-accents");
  const status = element("search-status");
  const results = element("results");
  let buttons: HTMLButtonElement[] = [];
  let current = -1;

  const show = (index: number) => {
    if (index < 0) return;
    buttons[current]?.classList.remove("current");
    current = index;
    const button = buttons[index];
    button.classList.add("current");
    button.scrollIntoView({ block: "nearest" });
    go(button.dataset.cfi!);
  };
  const search = searcher(view, {
    progress: (fraction) =>
      (status.textContent = `Searching… ${percent(fraction)}`),
    group: (group) => {
      const item = groupItem(group, (_, button) => {
        show(buttons.indexOf(button));
        panels.close();
      });
      buttons.push(...item.querySelectorAll("button"));
      results.append(item);
    },
    done: (count, capped) => (status.textContent = statusText(count, capped)),
  });
  const reset = () => {
    results.replaceChildren();
    buttons = [];
    current = -1;
    status.textContent = "";
  };
  const start = () => {
    reset();
    search
      .search(query.value, {
        matchWholeWords: wholeWords.checked,
        matchCase: matchCase.checked,
        matchDiacritics: matchAccents.checked,
      })
      .catch((error) => (status.textContent = `Search failed. ${error}`));
  };

  element("search")
    .querySelector("form")!
    .addEventListener("submit", (event) => {
      event.preventDefault();
      start();
    });
  for (const option of [wholeWords, matchCase, matchAccents])
    option.addEventListener("change", start);
  // Clearing the field, by hand or with its × button, clears the highlights.
  query.addEventListener("input", () => {
    if (!query.value) {
      search.clear();
      reset();
    }
  });

  return {
    find: () => {
      panels.open("search");
      query.focus();
      query.select();
    },
    step: (direction: 1 | -1) =>
      show(stepMatch(buttons.length, current, direction)),
  };
}

/** Each section's chapter title, from the nearest contents entry before it. */
function chapterTitles(view: View, book: Book) {
  const starts: { index: number; label: string }[] = [];
  const walk = (items: TocItem[]) => {
    for (const item of items) {
      const index = view.resolveNavigation(item.href)?.index;
      if (typeof index === "number" && index >= 0)
        starts.push({ index, label: item.label.trim() });
      if (item.subitems) walk(item.subitems);
    }
  };
  walk(book.toc ?? []);
  const topLevel = (book.toc ?? [])
    .map((item) => view.resolveNavigation(item.href)?.index)
    .filter(
      (index): index is number => typeof index === "number" && index >= 0,
    );
  return {
    of(section: number) {
      let label = "";
      for (const start of starts)
        if (start.index <= section && start.index >= 0) label = start.label;
      return label;
    },
    topLevel,
  };
}

/** The footer: back, chapter, page and time left, and the slider. */
function setUpFooter(
  view: View,
  book: Book,
  go: (turn: () => Promise<unknown>) => void,
) {
  const back = element<HTMLButtonElement>("back");
  const slider = element<HTMLInputElement>("slider");
  const progress = element("progress");
  let sliding = false;

  const history = view.history;
  const updateBack = () => (back.hidden = !history.canGoBack);
  history.addEventListener("index-change", updateBack);
  back.addEventListener("click", () => history.back());

  const marks = element("marks");
  const titles = chapterTitles(view, book);
  marks.replaceChildren(
    ...chapterMarks(titles.topLevel, view.getSectionFractions()).map((at) => {
      const mark = document.createElement("span");
      mark.style.left = `${at * 100}%`;
      return mark;
    }),
  );

  slider.addEventListener("input", () => {
    sliding = true;
    progress.textContent = percent(Number(slider.value) / SLIDER_STEPS);
  });
  slider.addEventListener("change", () => {
    sliding = false;
    const fraction = Number(slider.value) / SLIDER_STEPS;
    go(() => view.goToFraction(fraction));
  });

  return {
    titles,
    back: () => history.back(),
    forward: () => history.forward(),
    show(location: Location) {
      element("chapter").textContent = location.tocItem?.label ?? "";
      const { page, chapterLeft, bookLeft } = whereabouts(location);
      progress.textContent = [page, chapterLeft, percent(location.fraction)]
        .filter(Boolean)
        .join(" · ");
      progress.title = bookLeft ?? "";
      if (!sliding)
        slider.value = String(
          Math.round(
            Math.min(Math.max(location.fraction, 0), 1) * SLIDER_STEPS,
          ),
        );
      updateBack();
    },
  };
}

/** Notes that pop up over the page instead of sending the reader away. */
function setUpFootnotes(
  view: View,
  book: Book,
  styles: () => string,
  go: (target: string) => void,
  onFrame: (doc: Document) => void,
) {
  const handler = new FootnoteHandler();
  const box = element("footnote");
  const body = element("footnote-body");
  let shown: View | null = null;
  let href = "";

  const close = () => {
    if (box.hidden) return;
    box.hidden = true;
    shown?.close();
    shown = null;
    body.replaceChildren();
  };
  handler.addEventListener("before-render", (event) => {
    const { view: note } = (event as CustomEvent<{ view: View }>).detail;
    // In the page before it renders, so it has a size to lay out in.
    shown?.close();
    shown = note;
    element("footnote-title").textContent = "Note";
    body.replaceChildren(note);
    box.hidden = false;
    // Just the note: no neighbouring sections above or below it.
    note.renderer.setAttribute("flow", "scrolled");
    note.renderer.setAttribute("no-continuous-scroll", "");
    note.renderer.setAttribute("no-preload", "");
    note.renderer.setAttribute("gap", "4%");
    note.renderer.setStyles?.(styles());
    note.addEventListener("external-link", (e) => e.preventDefault());
    // A link inside the note goes in the book, not in the little window.
    note.addEventListener("link", (e) => {
      e.preventDefault();
      close();
      go((e as CustomEvent<LinkEvent>).detail.href);
    });
    note.addEventListener("load", (e) =>
      onFrame((e as CustomEvent<{ doc: Document }>).detail.doc),
    );
  });
  handler.addEventListener("render", (event) => {
    const detail = (event as CustomEvent<FootnoteRender>).detail;
    href = detail.href;
    element("footnote-title").textContent =
      detail.type === "endnote" ? "Endnote" : "Note";
  });
  view.addEventListener("link", (event) => {
    const { href: target } = (event as CustomEvent<LinkEvent>).detail;
    handler.handle(book, event)?.catch((error) => {
      // It looked like a note but wasn't one to show: just follow it.
      console.warn(error);
      close();
      go(target);
    });
  });
  element("footnote-close").addEventListener("click", close);
  element("footnote-go").addEventListener("click", () => {
    close();
    go(href);
  });
  return { close, isOpen: () => !box.hidden };
}

type MarkerTarget =
  | { kind: "new"; range: Range; index: number }
  | { kind: "existing"; highlight: Highlight; range?: Range };

/** Highlights and notes: on the page, in their panel, and stored by Rust. */
async function setUpHighlights(
  view: View,
  opened: OpenedBook,
  book: Book,
  titles: { of(section: number): string },
  go: (target: string) => void,
  readFrom: (range: Range) => void,
) {
  let list: Highlight[] = await loadHighlights().catch((error) => {
    console.error(error);
    return [];
  });
  const byCfi = () => new Map(list.map((h) => [h.cfi, h]));
  let index = byCfi();
  const sectionOf = (cfi: string) => {
    try {
      return view.resolveCFI(cfi).index;
    } catch {
      return -1;
    }
  };
  const chapterOf = (h: Highlight) => titles.of(sectionOf(h.cfi));

  const marker = element("marker");
  const noteForm = element<HTMLFormElement>("note-form");
  const noteField = element<HTMLTextAreaElement>("note");
  const removeButton = element("marker-remove");
  const status = element("notes-status");
  let target: MarkerTarget | null = null;
  /** A click that opened the toolbar mustn't also close it. */
  let justOpened = false;

  view.addEventListener("draw-annotation", (event) => {
    const { draw, annotation } = (event as CustomEvent<DrawAnnotation>).detail;
    const highlight = index.get(annotation.value);
    if (highlight)
      draw(Overlayer.highlight, { color: COLOR_VALUES[highlight.color] });
  });
  view.addEventListener("create-overlay", (event) => {
    const { index: section } = (event as CustomEvent<{ index: number }>).detail;
    for (const h of list)
      if (sectionOf(h.cfi) === section)
        view.addAnnotation({ value: h.cfi }).catch(console.error);
  });

  const renderList = () => {
    const holder = element("notes-list");
    if (!list.length) {
      status.textContent =
        "Select text on the page to highlight it or add a note.";
      holder.replaceChildren();
      return;
    }
    status.textContent =
      list.length === 1 ? "1 highlight" : `${list.length} highlights`;
    holder.replaceChildren(
      highlightList(list, chapterOf, (h) => {
        closeMarker();
        go(h.cfi);
      }),
    );
  };

  const clearSelections = () => {
    for (const { doc } of view.renderer.getContents?.() ?? [])
      doc.getSelection()?.removeAllRanges();
  };

  const closeMarker = () => {
    marker.hidden = true;
    noteForm.hidden = true;
    target = null;
  };

  const openMarker = (
    next: MarkerTarget,
    near: ReturnType<typeof boxInWindow>,
  ) => {
    target = next;
    justOpened = true;
    setTimeout(() => (justOpened = false));
    const color = next.kind === "existing" ? next.highlight.color : null;
    for (const swatch of marker.querySelectorAll<HTMLElement>("[data-color]"))
      swatch.setAttribute(
        "aria-checked",
        String(swatch.dataset.color === color),
      );
    removeButton.hidden = next.kind !== "existing";
    noteForm.hidden = true;
    marker.hidden = false;
    place(near);
  };
  let lastNear: ReturnType<typeof boxInWindow> | null = null;
  const place = (near: ReturnType<typeof boxInWindow>) => {
    lastNear = near;
    const { left, top } = placeNear(
      near,
      { width: marker.offsetWidth, height: marker.offsetHeight },
      { width: window.innerWidth, height: window.innerHeight },
    );
    marker.style.left = `${left}px`;
    marker.style.top = `${top}px`;
  };

  const replace = (saved: Highlight) => {
    list = [...list.filter((h) => h.id !== saved.id), saved];
    index = byCfi();
    view.addAnnotation({ value: saved.cfi }).catch(console.error);
    renderList();
  };

  const create = async (
    range: Range,
    section: number,
    color: HighlightColor,
  ) => {
    const cfi = view.getCFI(section, range);
    const existing = index.get(cfi);
    if (existing) return update(existing, { color });
    const fractions = view.getSectionFractions();
    const start = fractions[section] ?? 0;
    const end = fractions[section + 1] ?? start;
    const saved = await saveHighlight({
      cfi,
      href: book.sections[section]?.id ?? "",
      fraction: start + (end - start) * offsetIn(range),
      text: range.toString().slice(0, MAX_TEXT),
      note: null,
      color,
    });
    clearSelections();
    replace(saved);
    return saved;
  };
  const update = async (h: Highlight, changes: Partial<Highlight>) => {
    const { cfi, href, fraction, text, note, color } = { ...h, ...changes };
    const saved = await saveHighlight(
      { cfi, href, fraction, text, note, color },
      h.id,
    );
    replace(saved);
    return saved;
  };
  const remove = async (h: Highlight) => {
    await deleteHighlight(h.id);
    list = list.filter((x) => x.id !== h.id);
    index = byCfi();
    view.deleteAnnotation({ value: h.cfi }).catch(console.error);
    renderList();
  };
  const failed = (error: unknown) => {
    console.error(error);
    showMessage(`That highlight couldn't be saved. ${error}`);
    setTimeout(() => (element("message").hidden = true), 4_000);
  };

  /** The highlight the toolbar is for, made now if it's a fresh selection. */
  const current = async (color: HighlightColor = "yellow") => {
    if (!target) return null;
    if (target.kind === "existing") return target.highlight;
    const saved = await create(target.range, target.index, color);
    target = { kind: "existing", highlight: saved };
    return saved;
  };

  for (const swatch of marker.querySelectorAll<HTMLElement>("[data-color]")) {
    swatch.addEventListener("click", async () => {
      const color = swatch.dataset.color as HighlightColor;
      try {
        if (target?.kind === "existing")
          await update(target.highlight, { color });
        else await current(color);
        closeMarker();
      } catch (error) {
        failed(error);
      }
    });
  }
  element("marker-note").addEventListener("click", async () => {
    try {
      const h = await current();
      if (!h) return;
      removeButton.hidden = false;
      noteField.value = h.note ?? "";
      noteForm.hidden = false;
      // Taller now, so it may no longer fit above the text.
      if (lastNear) place(lastNear);
      noteField.focus();
    } catch (error) {
      failed(error);
    }
  });
  element("marker-speak").addEventListener("click", () => {
    const range = target?.range;
    closeMarker();
    clearSelections();
    if (range) readFrom(range);
  });
  removeButton.addEventListener("click", async () => {
    if (target?.kind !== "existing") return;
    const h = target.highlight;
    closeMarker();
    await remove(h).catch(failed);
  });
  noteForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (target?.kind !== "existing") return;
    const h = target.highlight;
    closeMarker();
    await update(h, { note: noteField.value.trim() || null }).catch(failed);
  });
  element("note-cancel").addEventListener("click", closeMarker);

  view.addEventListener("show-annotation", (event) => {
    const { value, range } = (event as CustomEvent<ShowAnnotation>).detail;
    const highlight = index.get(value);
    if (highlight)
      openMarker({ kind: "existing", highlight, range }, boxInWindow(range));
  });

  element("copy-notes").addEventListener("click", async () => {
    const text = asMarkdown(opened.title, list, chapterOf);
    status.textContent = (await copy(text))
      ? `Copied ${list.length === 1 ? "1 highlight" : `${list.length} highlights`}`
      : "Couldn't copy";
  });

  renderList();
  return {
    renderList,
    close: closeMarker,
    isOpen: () => !marker.hidden,
    /** Watches a page's frame for selections. */
    watch(doc: Document, section: number) {
      doc.addEventListener("pointerup", () =>
        setTimeout(() => {
          const selection = doc.getSelection();
          if (
            !selection ||
            selection.isCollapsed ||
            !selection.toString().trim()
          )
            return;
          const range = selection.getRangeAt(0);
          openMarker(
            { kind: "new", range, index: section },
            boxInWindow(range),
          );
        }),
      );
    },
    /** A click on the page closes the toolbar, unless it opened it. */
    pageClicked(doc: Document) {
      if (justOpened || marker.hidden) return;
      if (doc.getSelection()?.isCollapsed === false) return;
      closeMarker();
    },
  };
}

async function copy(text: string) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const field = document.createElement("textarea");
    field.value = text;
    document.body.append(field);
    field.select();
    const done = document.execCommand("copy");
    field.remove();
    return done;
  }
}

/** Read aloud, following along on the page. */
function setUpSpeech(view: View, book: Book) {
  const listen = element("listen");
  const bar = element("speech");
  const toggle = element("speech-toggle");
  const rate = element("speech-rate");
  if (!("speechSynthesis" in window)) {
    listen.hidden = true;
    return { readFrom: () => {}, stop: () => {} };
  }
  let from: Range | null = null;
  const follow = (range: Range) => {
    view.renderer.scrollToAnchor(range, true).catch(console.error);
  };
  const init = () => view.initTTS("sentence", undefined, follow);
  const source: SpeechSource = {
    async start() {
      await init();
      const tts = view.tts;
      if (!tts) return undefined;
      const start = from ?? view.lastLocation?.range ?? null;
      from = null;
      if (start) {
        try {
          const ssml = tts.from(start);
          if (ssml) return ssml;
        } catch {
          // A range from another section's page; start this one instead.
        }
      }
      return tts.start();
    },
    resume: () => view.tts?.resume(),
    async next() {
      let ssml = view.tts?.next();
      for (let i = 0; ssml === undefined && i < book.sections.length; i++) {
        const before = view.renderer.primaryIndex;
        await view.renderer.nextSection();
        if (view.renderer.primaryIndex === before) return undefined;
        await init();
        ssml = view.tts?.start();
      }
      return ssml;
    },
    mark: (name) => void view.tts?.setMark(name),
    stopped: () => {
      for (const { doc } of view.renderer.getContents?.() ?? [])
        doc.getSelection()?.removeAllRanges();
    },
  };
  const speaker = new Speaker(source, (state) => {
    listen.setAttribute("aria-pressed", String(state !== "stopped"));
    bar.hidden = state === "stopped";
    toggle.textContent = state === "playing" ? "Pause" : "Play";
  });
  listen.addEventListener("click", () => {
    if (speaker.state === "stopped") void speaker.play();
    else speaker.stop();
  });
  toggle.addEventListener("click", () => {
    if (speaker.state === "playing") speaker.pause();
    else void speaker.play();
  });
  rate.addEventListener("click", () => {
    speaker.setRate(nextRate(speaker.rate));
    rate.textContent = `${speaker.rate}×`;
  });
  element("speech-stop").addEventListener("click", () => speaker.stop());
  window.addEventListener("pagehide", () => speaker.stop());
  return {
    readFrom(range: Range) {
      speaker.stop();
      from = range;
      void speaker.play();
    },
    stop: () => speaker.stop(),
  };
}

async function read(opened: OpenedBook) {
  document.title = opened.title;
  element("title").textContent = opened.title;

  const book = await new EPUB(bookLoader(opened.entries)).init();
  const view = document.createElement("foliate-view") as View;
  element("book").append(view);
  await view.open(book);

  // Made once the frames' listeners are ready; page turns close its toolbar.
  const marks: { current?: Awaited<ReturnType<typeof setUpHighlights>> } = {};
  const turnWith = (turn: () => Promise<unknown>) => {
    marks.current?.close();
    within(turn(), TURN_TIMEOUT_MS).catch(console.error);
  };
  const go = (target: string) => turnWith(() => view.goTo(target));
  const next = () => turnWith(() => view.next());
  const previous = () => turnWith(() => view.prev());

  const appearance = setUpAppearance(view, opened.settings);
  const panels = setUpPanels((name) => {
    if (name === "search") element("query").focus();
    if (name === "notes") marks.current?.renderList();
  });
  const finder = setUpSearch(view, panels, go);
  const footer = setUpFooter(view, book, turnWith);
  const speech = setUpSpeech(view, book);

  const closeAll = () => {
    panels.close();
    marks.current?.close();
    footnotes.close();
  };
  const onKey = (event: KeyboardEvent) => {
    const actions = {
      left: () => turnWith(() => view.goLeft()),
      right: () => turnWith(() => view.goRight()),
      next,
      previous,
      larger: () => appearance.resize(1),
      smaller: () => appearance.resize(-1),
      close: closeAll,
      find: finder.find,
      findNext: () => finder.step(1),
      findPrevious: () => finder.step(-1),
      back: footer.back,
      forward: footer.forward,
    };
    const target = event.target as HTMLElement | null;
    const typing = !!target?.closest?.("input, textarea, select");
    const action = keyAction({
      key: event.key,
      shiftKey: event.shiftKey,
      metaKey: event.metaKey,
      ctrlKey: event.ctrlKey,
      altKey: event.altKey,
      typing,
    });
    if (!action) return;
    event.preventDefault();
    actions[action]();
  };
  const pageWheel = wheelPager((direction) =>
    direction > 0 ? next() : previous(),
  );
  // Scrolling the text scrolls it; only pages turn on a scroll gesture.
  const onWheel = (event: WheelEvent) => {
    if (!appearance.scrolled()) pageWheel(event);
  };
  const onFrame = (doc: Document) => doc.addEventListener("keydown", onKey);

  const footnotes = setUpFootnotes(view, book, appearance.styles, go, onFrame);
  const highlights = await setUpHighlights(
    view,
    opened,
    book,
    footer.titles,
    go,
    speech.readFrom,
  );
  marks.current = highlights;

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
    const { doc, index } = (
      event as CustomEvent<{ doc: Document; index: number }>
    ).detail;
    onFrame(doc);
    doc.addEventListener("wheel", onWheel, { passive: true });
    doc.addEventListener("click", () => {
      panels.close();
      footnotes.close();
      highlights.pageClicked(doc);
    });
    highlights.watch(doc, index);
  });
  // No opening the web from a book. Cancelling stops the engine doing so.
  view.addEventListener("external-link", (event) => event.preventDefault());

  const toc = element("toc");
  toc.replaceChildren(
    contentsList(book.toc ?? [], (href) => {
      panels.close();
      go(href);
    }),
  );

  await resume(view, opened.position);

  const remember = positionSaver(book.sections);
  const show = (location: Location) => {
    footer.show(location);
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
