import {
  comicPage,
  deleteHighlight,
  highlights,
  onMenuCommand,
  onSettingsPush,
  saveHighlight,
  saveLatestPosition,
  saveSettings,
  type ComicBook,
  type Highlight,
  type OpenedBook,
  type Theme,
} from "./backend";
import { shownTheme } from "./appearance";
import { keyAction, wheelPager } from "./paging";
import { appTheme, isDark } from "../design/theme";

type Layout = "auto" | "single" | "scroll";
type Fit = "page" | "width" | "height";
type PageRequest = Promise<{ img: HTMLImageElement; url: string }>;
const element = <T extends HTMLElement = HTMLElement>(id: string) =>
  document.getElementById(id) as T;
const reference = (page: number) => `comic-page/${page}`;
const pageIndex = (href: string | undefined, count: number) => {
  const match = href?.match(/^comic-page\/(\d+)$/);
  if (!match) return null;
  const index = Number(match[1]);
  return Number.isInteger(index) && index >= 0 && index < count ? index : null;
};

/** Face ordinary portrait pages in archive order and show the cover alone. */
function spreads(comic: ComicBook, pair: boolean) {
  const result: number[][] = [];
  const ordinary = (i: number) => {
    const page = comic.pages[i];
    return (
      page &&
      i !== 0 &&
      i !== comic.cover &&
      page.width < page.height &&
      page.height < 2.5 * page.width
    );
  };
  for (let i = 0; i < comic.pages.length;) {
    const group = [i++];
    if (pair && ordinary(group[0]) && ordinary(i)) group.push(i++);
    result.push(group);
  }
  return result;
}

function setupTheme(opened: OpenedBook) {
  let settings = opened.settings;
  let app = opened.appTheme;
  const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
  const choices = document.querySelectorAll<HTMLButtonElement>(
    '[data-setting="theme"]',
  );
  const apply = () => {
    const theme = shownTheme(settings.theme, app, systemDark.matches);
    document.documentElement.dataset.theme = theme;
    document.documentElement.style.colorScheme = isDark(theme)
      ? "dark"
      : "light";
    for (const choice of choices) {
      choice.setAttribute(
        "aria-checked",
        String(settings.theme === choice.dataset.value),
      );
    }
  };
  systemDark.addEventListener("change", apply);
  onSettingsPush((push) => {
    if (push.settings) settings = { ...settings, ...push.settings };
    if (push.appTheme) app = appTheme(push.appTheme);
    apply();
  });
  for (const choice of choices) {
    choice.addEventListener("click", () => {
      settings = { ...settings, theme: choice.dataset.value as Theme };
      apply();
      void saveSettings(settings).catch(console.error);
    });
  }
  apply();
}

/** The image reader uses only raster entries and never creates a book frame. */
export async function readComic(opened: OpenedBook, comic: ComicBook) {
  document.title = opened.title;
  element("title").textContent = opened.title;
  document.body.classList.add("comic-reader");
  const hasChapters = comic.chapters.length > 0;
  const contentsLabel = hasChapters ? "Contents" : "Bookmarks";
  element("contents").textContent = contentsLabel;
  element("toc").setAttribute("aria-label", contentsLabel);
  element("appearance").textContent = "View";
  element("appearance").setAttribute("aria-label", "View");
  element("appearance").title = "View";
  element("settings").setAttribute("aria-label", "View");
  document.querySelector<HTMLElement>(".comic-controls")!.hidden = false;
  element("comic-status").hidden = false;
  for (const id of ["find", "listen", "notes-button"])
    element(id).hidden = true;
  setupTheme(opened);

  const viewer = element("book");
  const sheet = document.createElement("div");
  sheet.className = "comic-sheet";
  viewer.append(sheet);
  const rtl = comic.direction === "rtl";
  element("chapter").textContent = rtl ? "Right to left" : "Left to right";
  const count = comic.pages.length;
  const slider = element<HTMLInputElement>("slider");
  slider.max = String(count - 1);
  slider.dir = rtl ? "rtl" : "ltr";
  const layoutControl = element<HTMLSelectElement>("comic-layout");
  const fitControl = element<HTMLSelectElement>("comic-fit");
  const markButton = element<HTMLButtonElement>("comic-bookmark");
  markButton.hidden = false;
  const left = element<HTMLButtonElement>("prev");
  const right = element<HTMLButtonElement>("next");
  left.setAttribute("aria-label", rtl ? "Next page" : "Previous page");
  right.setAttribute("aria-label", rtl ? "Previous page" : "Next page");

  const stripCount = comic.pages.filter(
    (p) => p.width > 0 && p.height >= 2.5 * p.width,
  ).length;
  // Vertical scroll is the default only for mostly long strips. The book
  // reader's scrolled flow is a reading preference for text, not comics.
  let layout: Layout =
    stripCount * 2 > count ? "scroll" : opened.settings.spread;
  layoutControl.value = layout;
  let fit: Fit = "page";
  let zoom = 1;
  let page = pageIndex(opened.position?.href, count) ?? 0;
  let groups: number[][] = [];
  let groupOf: number[] = [];
  let figures = new Map<number, HTMLElement>();
  let lastRendered: string | null = null;
  const cache = new Map<number, PageRequest>();
  let loadQueue = Promise.resolve();
  const marks = new Map<number, Highlight>();
  let togglingMark = false;
  const save = saveLatestPosition();
  let lastSize = "";

  const scrollMode = () => layout === "scroll";
  const fraction = (i: number) => (count > 1 ? i / (count - 1) : 0);
  const currentGroup = () => groups[groupOf[page]] ?? [page];

  const remember = () =>
    save({
      href: reference(page),
      cfi: "",
      fraction: fraction(page),
      quote: null,
    });

  function release(request: PageRequest) {
    void request.then(
      ({ url }) => URL.revokeObjectURL(url),
      () => {},
    );
  }

  function load(i: number) {
    let request = cache.get(i);
    if (!request) {
      const queued: PageRequest = loadQueue.then(async () => {
        if (cache.get(i) !== queued) throw new Error("page no longer visible");
        const bytes = await comicPage(i);
        if (cache.get(i) !== queued) throw new Error("page no longer visible");
        const url = URL.createObjectURL(bytes);
        try {
          const img = new Image();
          img.alt = `Page ${i + 1}`;
          img.draggable = false;
          img.src = url;
          await img.decode();
          return { img, url };
        } catch (error) {
          URL.revokeObjectURL(url);
          throw error;
        }
      });
      cache.set(i, queued);
      loadQueue = queued.then(
        () => {},
        () => {},
      );
      request = queued;
    }
    return request;
  }

  const gap = Number.parseFloat(getComputedStyle(sheet).columnGap) || 0;

  function size(figure: HTMLElement, i: number, columns: number) {
    const width = comic.pages[i]?.width || 3;
    const height = comic.pages[i]?.height || 4;
    const spacing = gap * (columns - 1);
    const horizontal =
      Math.max(1, viewer.clientWidth - spacing) / (columns * width);
    const vertical = Math.max(1, viewer.clientHeight) / height;
    const base =
      scrollMode() || fit === "width"
        ? horizontal
        : fit === "height"
          ? vertical
          : Math.min(horizontal, vertical);
    const scale = base * zoom;
    figure.style.width = `${Math.max(1, Math.floor(width * scale))}px`;
    figure.style.height = `${Math.max(1, Math.floor(height * scale))}px`;
  }

  function figure(i: number, columns: number) {
    const cell = document.createElement("figure");
    cell.className = "comic-page";
    cell.dataset.page = String(i);
    size(cell, i, columns);
    return cell;
  }

  function pageAt(top: number) {
    let lo = 0;
    let hi = count;
    while (lo + 1 < hi) {
      const middle = Math.floor((lo + hi) / 2);
      if (figures.get(middle)!.offsetTop <= top) lo = middle;
      else hi = middle;
    }
    return lo;
  }

  function keepPages() {
    const start = scrollMode() ? Math.max(0, pageAt(viewer.scrollTop) - 1) : 0;
    const end = scrollMode()
      ? Math.max(page, pageAt(viewer.scrollTop + viewer.clientHeight))
      : 0;
    const visible = scrollMode()
      ? Array.from({ length: end - start + 1 }, (_, i) => start + i)
      : currentGroup();
    const last = visible.at(-1) ?? page;
    const kept = new Set(
      [...visible, last + 1, last + 2].filter((i) => i < count),
    );
    for (const [i, request] of cache) {
      if (kept.has(i)) continue;
      if (scrollMode()) figures.get(i)?.replaceChildren();
      cache.delete(i);
      release(request);
    }
    for (const i of kept) {
      const cell = figures.get(i);
      const request = load(i);
      if (!cell || cell.firstChild) continue;
      void request.then(
        ({ img }) => {
          if (figures.get(i) === cell && cache.get(i) === request)
            cell.replaceChildren(img);
        },
        () => {
          if (figures.get(i) === cell && cache.get(i) === request) {
            cell.textContent = `Page ${i + 1} could not be opened.`;
          }
        },
      );
    }
  }

  function updateUI() {
    const group = scrollMode() ? [page] : currentGroup();
    const label =
      group.length > 1
        ? `Pages ${group[0] + 1}–${group.at(-1)! + 1}`
        : `Page ${page + 1}`;
    element("progress").textContent = `${label} of ${count}`;
    slider.value = String(page);
    slider.setAttribute("aria-valuetext", `Page ${page + 1} of ${count}`);
    markButton.setAttribute("aria-pressed", String(marks.has(page)));
    markButton.setAttribute(
      "aria-label",
      marks.has(page) ? "Remove page bookmark" : "Bookmark page",
    );
    markButton.title = marks.has(page)
      ? "Remove page bookmark"
      : "Bookmark page";
    markButton.disabled = togglingMark;
    const back = group[0] === 0;
    const forward = group.at(-1) === count - 1;
    left.disabled = rtl ? forward : back;
    right.disabled = rtl ? back : forward;
    element("comic-zoom").textContent = `${Math.round(zoom * 100)}%`;
  }

  function renderPaged() {
    const group = currentGroup();
    const key = group.join(",");
    if (key !== lastRendered) {
      lastRendered = key;
      const order = rtl ? [...group].reverse() : group;
      figures = new Map(order.map((i) => [i, figure(i, group.length)]));
      sheet.replaceChildren(...figures.values());
      viewer.scrollTo(0, 0);
    } else {
      for (const [i, cell] of figures) size(cell, i, group.length);
    }
    keepPages();
  }

  function go(i: number) {
    if (i < 0 || i >= count) return;
    page = i;
    if (scrollMode()) {
      viewer.scrollTop = figures.get(page)?.offsetTop ?? 0;
      keepPages();
    } else {
      renderPaged();
    }
    updateUI();
    remember();
  }

  function relayout() {
    const scrolled = scrollMode();
    viewer.classList.toggle("comic-scroll", scrolled);
    left.hidden = scrolled;
    right.hidden = scrolled;
    element("stage").classList.toggle("comic-scroll-stage", scrolled);
    fitControl.disabled = scrolled;
    if (scrolled) {
      lastRendered = null;
      figures = new Map(comic.pages.map((_, i) => [i, figure(i, 1)]));
      sheet.replaceChildren(...figures.values());
      const last = figures.get(count - 1)?.offsetHeight ?? 0;
      sheet.style.setProperty(
        "--comic-tail",
        `${Math.max(0, viewer.clientHeight - last)}px`,
      );
      viewer.scrollTop = figures.get(page)?.offsetTop ?? 0;
      keepPages();
    } else {
      groups = spreads(comic, layout === "auto" && viewer.clientWidth >= 740);
      groupOf = [];
      groups.forEach((group, i) =>
        group.forEach((p) => {
          groupOf[p] = i;
        }),
      );
      renderPaged();
    }
    lastSize = `${viewer.clientWidth}x${viewer.clientHeight}`;
    updateUI();
  }

  function turn(direction: 1 | -1) {
    if (scrollMode()) go(page + direction);
    else {
      const next = groups[groupOf[page] + direction]?.[0];
      if (next !== undefined) go(next);
    }
  }

  function stepZoom(direction: 1 | -1) {
    zoom = Math.max(0.5, Math.min(4, zoom + direction * 0.25));
    relayout();
  }

  const toc = element("toc");
  const panel = element("settings");
  function openPanel(which?: "toc" | "settings") {
    for (const [id, button] of [
      ["toc", "contents"],
      ["settings", "appearance"],
    ]) {
      const node = element(id);
      node.hidden = which !== id;
      element(button).setAttribute("aria-expanded", String(!node.hidden));
    }
  }

  function navigationList(rows: ComicBook["chapters"], label: string) {
    const list = document.createElement("ol");
    list.setAttribute("aria-label", label);
    for (const { page: i, title } of rows) {
      const item = document.createElement("li");
      const button = document.createElement("button");
      button.textContent = title;
      button.addEventListener("click", () => {
        openPanel();
        go(i);
      });
      item.append(button);
      list.append(item);
    }
    return list;
  }

  function renderMarks() {
    toc.replaceChildren();
    if (hasChapters) {
      toc.append(navigationList(comic.chapters, "Chapters"));
      const heading = document.createElement("p");
      heading.textContent = "Bookmarks";
      toc.append(heading);
    }
    if (marks.size) {
      const rows = [...marks.keys()]
        .sort((a, b) => a - b)
        .map((page) => ({ page, title: `Page ${page + 1}` }));
      toc.append(navigationList(rows, "Bookmarks"));
    } else {
      const empty = document.createElement("p");
      empty.textContent = "No bookmarks yet.";
      toc.append(empty);
    }
  }

  const markStatus = element("comic-status");
  try {
    for (const mark of await highlights()) {
      const i = pageIndex(mark.href, count);
      if (i !== null) marks.set(i, mark);
    }
  } catch (error) {
    markStatus.textContent = String(error);
  }
  renderMarks();
  markButton.addEventListener("click", async () => {
    const i = page;
    const existing = marks.get(i);
    togglingMark = true;
    markStatus.textContent = "";
    updateUI();
    try {
      if (existing) {
        await deleteHighlight(existing.id);
        marks.delete(i);
      } else {
        marks.set(
          i,
          await saveHighlight({
            href: reference(i),
            cfi: reference(i),
            fraction: fraction(i),
            text: `Page ${i + 1}`,
            note: null,
            color: "yellow",
          }),
        );
      }
      renderMarks();
    } catch (error) {
      markStatus.textContent = String(error);
    } finally {
      togglingMark = false;
      updateUI();
    }
  });
  element("contents").addEventListener("click", () =>
    openPanel(toc.hidden ? "toc" : undefined),
  );
  element("appearance").addEventListener("click", () =>
    openPanel(panel.hidden ? "settings" : undefined),
  );
  layoutControl.addEventListener("change", () => {
    layout = layoutControl.value as Layout;
    relayout();
  });
  fitControl.addEventListener("change", () => {
    fit = fitControl.value as Fit;
    relayout();
  });
  element("comic-minus").addEventListener("click", () => stepZoom(-1));
  element("comic-plus").addEventListener("click", () => stepZoom(1));
  element("comic-reset").addEventListener("click", () => {
    zoom = 1;
    relayout();
  });
  slider.addEventListener("change", () => go(Number(slider.value)));
  left.addEventListener("click", () => turn(rtl ? 1 : -1));
  right.addEventListener("click", () => turn(rtl ? -1 : 1));

  const actions = {
    left: () => turn(rtl ? 1 : -1),
    right: () => turn(rtl ? -1 : 1),
    next: () => turn(1),
    previous: () => turn(-1),
    larger: () => stepZoom(1),
    smaller: () => stepZoom(-1),
    close: () => openPanel(),
  };
  window.addEventListener("keydown", (event) => {
    const target = event.target as HTMLElement | null;
    if (event.key === " " && target?.closest?.("button")) return;
    const typing = !!target?.closest?.("input, select");
    const action = keyAction({
      key: event.key,
      shiftKey: event.shiftKey,
      metaKey: event.metaKey,
      ctrlKey: event.ctrlKey,
      altKey: event.altKey,
      typing,
    });
    if (!action || !(action in actions)) return;
    event.preventDefault();
    if (scrollMode() && (action === "next" || action === "previous")) {
      const step =
        viewer.clientHeight * (event.key.startsWith("Arrow") ? 0.1 : 0.85);
      viewer.scrollBy(0, action === "next" ? step : -step);
    } else {
      actions[action as keyof typeof actions]();
    }
  });
  onMenuCommand((command) => {
    switch (command) {
      case "book.contents":
      case "book.notes":
        openPanel("toc");
        break;
      case "book.larger":
        stepZoom(1);
        break;
      case "book.smaller":
        stepZoom(-1);
        break;
      case "book.back":
        turn(-1);
        break;
      case "book.forward":
        turn(1);
        break;
    }
  });

  const panEnabled = () =>
    viewer.scrollWidth > viewer.clientWidth ||
    viewer.scrollHeight > viewer.clientHeight;
  let drag: {
    id: number;
    x: number;
    y: number;
    left: number;
    top: number;
  } | null = null;
  let moved = false;
  viewer.addEventListener("pointerdown", (event) => {
    moved = false;
    if (scrollMode() || !panEnabled() || event.button !== 0) return;
    drag = {
      id: event.pointerId,
      x: event.clientX,
      y: event.clientY,
      left: viewer.scrollLeft,
      top: viewer.scrollTop,
    };
  });
  viewer.addEventListener("pointermove", (event) => {
    if (!drag || drag.id !== event.pointerId) return;
    const dx = event.clientX - drag.x;
    const dy = event.clientY - drag.y;
    if (!moved && Math.hypot(dx, dy) < 5) return;
    if (!moved) viewer.setPointerCapture(drag.id);
    moved = true;
    viewer.scrollTo(drag.left - dx, drag.top - dy);
  });
  for (const event of ["pointerup", "pointercancel", "lostpointercapture"]) {
    viewer.addEventListener(event, () => {
      drag = null;
    });
  }
  viewer.addEventListener("click", (event) => {
    openPanel();
    if (scrollMode() || moved) return;
    const rect = viewer.getBoundingClientRect();
    const onLeft = event.clientX < rect.left + rect.width / 2;
    turn(onLeft === rtl ? 1 : -1);
  });
  const wheel = wheelPager(turn);
  viewer.addEventListener(
    "wheel",
    (event) => {
      if (!scrollMode() && !panEnabled()) wheel(event);
    },
    { passive: true },
  );

  viewer.addEventListener(
    "scroll",
    () => {
      if (!scrollMode()) return;
      const visiblePage = pageAt(viewer.scrollTop + 2);
      if (visiblePage === page) {
        keepPages();
        return;
      }
      page = visiblePage;
      keepPages();
      updateUI();
      remember();
    },
    { passive: true },
  );
  new ResizeObserver(() => {
    const measured = `${viewer.clientWidth}x${viewer.clientHeight}`;
    if (measured !== lastSize) relayout();
  }).observe(viewer);
  window.addEventListener("pagehide", () => {
    for (const request of cache.values()) release(request);
    cache.clear();
  });

  relayout();
}
