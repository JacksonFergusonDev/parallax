// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import type { SearchResult } from "foliate-js/view.js";
import {
  MAX_MATCHES,
  groupItem,
  searcher,
  statusText,
  stepMatch,
} from "./search";

const match = (n: number) => ({
  cfi: `epubcfi(/6/2!/4/${n})`,
  excerpt: { pre: "before ", match: `hit${n}`, post: " after" },
});

function fakeView(results: (SearchResult | "done")[], gate?: Promise<void>) {
  const clearSearch = vi.fn();
  return {
    clearSearch,
    async *search() {
      for (const result of results) {
        if (gate) await gate;
        yield result;
      }
    },
  };
}

const listener = () => ({ group: vi.fn(), progress: vi.fn(), done: vi.fn() });

describe("searcher", () => {
  it("reports each chapter's matches, then the total", async () => {
    const view = fakeView([
      { progress: 0.5 },
      { index: 1, label: "Two", subitems: [match(1), match(2)] },
      "done",
    ]);
    const listen = listener();
    await searcher(view, listen).search("hit", {});
    expect(listen.progress).toHaveBeenCalledWith(0.5);
    expect(listen.group).toHaveBeenCalledWith({
      label: "Two",
      matches: [match(1), match(2)],
    });
    expect(listen.done).toHaveBeenCalledWith(2, false);
  });

  it("stops at the cap", async () => {
    const many = Array.from({ length: MAX_MATCHES + 50 }, (_, i) => match(i));
    const listen = listener();
    await searcher(
      fakeView([{ index: 0, label: "", subitems: many }]),
      listen,
    ).search("hit", {});
    expect(listen.group.mock.calls[0][0].matches).toHaveLength(MAX_MATCHES);
    expect(listen.done).toHaveBeenCalledWith(MAX_MATCHES, true);
  });

  it("drops a search that a newer one replaced", async () => {
    let release = () => {};
    const gate = new Promise<void>((resolve) => (release = resolve));
    const view = fakeView(
      [{ index: 0, label: "Old", subitems: [match(1)] }],
      gate,
    );
    const listen = listener();
    const search = searcher(view, listen);
    const old = search.search("old", {});
    search.clear();
    release();
    await old;
    expect(listen.group).not.toHaveBeenCalled();
    expect(listen.done).not.toHaveBeenCalled();
    expect(view.clearSearch).toHaveBeenCalledTimes(2);
  });

  it("treats a blank query as clearing", async () => {
    const view = fakeView([]);
    const listen = listener();
    await searcher(view, listen).search("   ", {});
    expect(view.clearSearch).toHaveBeenCalled();
    expect(listen.done).not.toHaveBeenCalled();
  });
});

describe("results", () => {
  it("shows excerpts from the book as text, not markup", () => {
    const item = groupItem(
      {
        label: "<b>Chapter</b>",
        matches: [
          { cfi: "x", excerpt: { pre: "<img src=x>", match: "hit", post: "" } },
        ],
      },
      vi.fn(),
    );
    expect(item.querySelector("img, b")).toBeNull();
    expect(item.querySelector("mark")?.textContent).toBe("hit");
    expect(item.querySelector("button")?.textContent).toBe("<img src=x>hit");
  });

  it("goes to the match chosen", () => {
    const choose = vi.fn();
    const item = groupItem({ label: "One", matches: [match(1)] }, choose);
    item.querySelector("button")!.click();
    expect(choose.mock.calls[0][0]).toEqual(match(1));
  });

  it("describes the count", () => {
    expect(statusText(0, false)).toBe("No matches");
    expect(statusText(1, false)).toBe("1 match");
    expect(statusText(7, false)).toBe("7 matches");
    expect(statusText(MAX_MATCHES, true)).toContain("first");
  });
});

describe("stepMatch", () => {
  it("wraps round both ways", () => {
    expect(stepMatch(3, -1, 1)).toBe(0);
    expect(stepMatch(3, -1, -1)).toBe(2);
    expect(stepMatch(3, 2, 1)).toBe(0);
    expect(stepMatch(3, 0, -1)).toBe(2);
    expect(stepMatch(0, -1, 1)).toBe(-1);
  });
});
