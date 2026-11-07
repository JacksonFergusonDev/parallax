import type { TocItem } from "foliate-js/epub.js";

/** The book's contents as nested lists of buttons. */
export function contentsList(
  items: TocItem[],
  choose: (href: string) => void,
): HTMLOListElement {
  const list = document.createElement("ol");
  for (const item of items) {
    const entry = document.createElement("li");
    const button = document.createElement("button");
    // textContent, not innerHTML: labels come from the book.
    button.textContent = item.label;
    button.dataset.href = item.href;
    button.addEventListener("click", () => choose(item.href));
    entry.append(button);
    if (item.subitems?.length) {
      entry.append(contentsList(item.subitems, choose));
    }
    list.append(entry);
  }
  return list;
}

/** Marks where the reader is, and returns that entry to scroll to. */
export function markCurrent(root: HTMLElement, href: string | undefined) {
  let current: HTMLElement | undefined;
  for (const button of root.querySelectorAll("button")) {
    const here = button.dataset.href === href;
    button.classList.toggle("current", here);
    if (here) current = button;
  }
  return current;
}
