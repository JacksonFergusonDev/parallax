import type {
  SearchMatch,
  SearchOptions,
  SearchResult,
} from "foliate-js/view.js";

/** Common words match thousands of times; past this the list stops growing. */
export const MAX_MATCHES = 500;

export interface Group {
  label: string;
  matches: SearchMatch[];
}

interface Searchable {
  search(options: SearchOptions): AsyncGenerator<SearchResult | "done">;
  clearSearch(): void;
}

interface Listener {
  group(group: Group): void;
  progress(fraction: number): void;
  done(count: number, capped: boolean): void;
}

/**
 * Runs one search at a time: starting another, or clearing, abandons the one
 * in progress, so its late results never mix into the new list.
 */
export function searcher(view: Searchable, listen: Listener) {
  let run = 0;
  return {
    async search(query: string, options: Omit<SearchOptions, "query">) {
      const mine = ++run;
      view.clearSearch();
      if (!query.trim()) return;
      let count = 0;
      let capped = false;
      for await (const result of view.search({ query, ...options })) {
        if (mine !== run || result === "done") break;
        if ("progress" in result) listen.progress(result.progress);
        else {
          const matches = result.subitems.slice(0, MAX_MATCHES - count);
          count += matches.length;
          listen.group({ label: result.label, matches });
          if (count >= MAX_MATCHES) {
            capped = true;
            break;
          }
        }
      }
      if (mine === run) listen.done(count, capped);
    },
    clear() {
      run++;
      view.clearSearch();
    },
  };
}

export function statusText(count: number, capped: boolean): string {
  if (capped) return `Showing the first ${MAX_MATCHES} matches`;
  if (count === 0) return "No matches";
  return count === 1 ? "1 match" : `${count} matches`;
}

/** One chapter's matches. Excerpts come from the book, so they're text only. */
export function groupItem(
  group: Group,
  choose: (match: SearchMatch, button: HTMLButtonElement) => void,
): HTMLLIElement {
  const item = document.createElement("li");
  const heading = document.createElement("h2");
  heading.textContent = group.label || "Untitled section";
  const list = document.createElement("ol");
  for (const match of group.matches) {
    const entry = document.createElement("li");
    const button = document.createElement("button");
    const mark = document.createElement("mark");
    mark.textContent = match.excerpt.match;
    button.append(match.excerpt.pre, mark, match.excerpt.post);
    button.dataset.cfi = match.cfi;
    button.addEventListener("click", () => choose(match, button));
    entry.append(button);
    list.append(entry);
  }
  item.append(heading, list);
  return item;
}

/** The match `step` away from the current one, wrapping round at the ends. */
export function stepMatch(count: number, current: number, step: 1 | -1) {
  if (count === 0) return -1;
  if (current < 0) return step > 0 ? 0 : count - 1;
  return (current + step + count) % count;
}
