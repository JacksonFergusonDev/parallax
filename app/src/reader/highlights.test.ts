// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import type { Highlight } from "./backend";
import {
  asMarkdown,
  highlightList,
  inReadingOrder,
  offsetIn,
  placeNear,
  shortened,
} from "./highlights";

const highlight = (
  id: number,
  fraction: number,
  more: Partial<Highlight> = {},
): Highlight => ({
  id,
  fraction,
  cfi: `epubcfi(/6/${id})`,
  href: "c1.xhtml",
  text: `Text ${id}`,
  note: null,
  color: "yellow",
  createdAt: "",
  updatedAt: "",
  ...more,
});

describe("highlights", () => {
  it("keep reading order, then the order they were made", () => {
    const list = [highlight(3, 0.5), highlight(1, 0.9), highlight(2, 0.5)];
    expect(inReadingOrder(list).map((h) => h.id)).toEqual([2, 3, 1]);
  });

  it("show long passages cut short, on one line", () => {
    expect(shortened("a\n  b")).toBe("a b");
    expect(shortened("abcdefghij", 6)).toBe("abcde…");
  });

  it("list book text as text, never markup", () => {
    const choose = vi.fn();
    const list = highlightList(
      [
        highlight(1, 0.1, {
          text: "<img src=x onerror=alert(1)>",
          note: "<b>mine</b>",
        }),
      ],
      () => "Chapter 1",
      choose,
    );
    expect(list.querySelector("img, b")).toBeNull();
    expect(list.querySelector(".quote")!.textContent).toBe(
      "<img src=x onerror=alert(1)>",
    );
    expect(list.querySelector(".note")!.textContent).toBe("<b>mine</b>");
    list.querySelector("button")!.click();
    expect(choose).toHaveBeenCalledWith(expect.objectContaining({ id: 1 }));
  });

  it("copy out as Markdown, grouped by chapter", () => {
    const chapters: Record<number, string> = { 1: "One", 2: "One", 3: "Two" };
    const md = asMarkdown(
      "Emma",
      [
        highlight(3, 0.7),
        highlight(1, 0.1, { note: "Why?" }),
        highlight(2, 0.2, { text: "A\nB" }),
      ],
      (h) => chapters[h.id],
    );
    expect(md).toBe(
      "# Emma\n\n## One\n\n> Text 1\n\nWhy?\n\n> A B\n\n## Two\n\n> Text 3\n",
    );
  });

  it("measure how far into a page a selection starts", () => {
    document.body.innerHTML = "<p>abcde</p><p>fghij</p>";
    const range = document.createRange();
    range.setStart(document.querySelectorAll("p")[1].firstChild!, 0);
    expect(offsetIn(range)).toBe(0.5);
  });
});

describe("placing the toolbar", () => {
  const size = { width: 200, height: 40 };
  const viewport = { width: 600, height: 800 };

  it("goes centred above the selection", () => {
    expect(
      placeNear(
        { left: 200, right: 300, top: 300, bottom: 320 },
        size,
        viewport,
      ),
    ).toEqual({
      left: 150,
      top: 252,
    });
  });

  it("goes below when there's no room above, and stays in the window", () => {
    expect(
      placeNear({ left: 0, right: 20, top: 10, bottom: 30 }, size, viewport),
    ).toEqual({
      left: 8,
      top: 38,
    });
    expect(
      placeNear(
        { left: 590, right: 600, top: 10, bottom: 790 },
        size,
        viewport,
      ),
    ).toEqual({
      left: 392,
      top: 752,
    });
  });
});
