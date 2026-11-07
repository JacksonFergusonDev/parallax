// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { contentsList, markCurrent } from "./contents";

const items = [
  {
    label: "One",
    href: "a.xhtml",
    subitems: [{ label: "<b>Two</b>", href: "b.xhtml" }],
  },
];

describe("contentsList", () => {
  it("nests entries and goes to the one chosen", () => {
    const choose = vi.fn();
    const list = contentsList(items, choose);
    expect(list.querySelectorAll("ol ol button")).toHaveLength(1);
    list.querySelectorAll("button")[1].click();
    expect(choose).toHaveBeenCalledWith("b.xhtml");
  });

  it("treats labels from the book as text", () => {
    const list = contentsList(items, vi.fn());
    expect(list.querySelector("b")).toBeNull();
    expect(list.querySelectorAll("button")[1].textContent).toBe("<b>Two</b>");
  });

  it("marks only the current entry", () => {
    const list = contentsList(items, vi.fn());
    expect(markCurrent(list, "a.xhtml")?.textContent).toBe("One");
    expect(markCurrent(list, "b.xhtml")?.textContent).toBe("<b>Two</b>");
    expect(list.querySelectorAll(".current")).toHaveLength(1);
  });
});
