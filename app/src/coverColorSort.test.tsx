// @vitest-environment jsdom
import { act, useMemo } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { coverColors } from "./api";
import {
  colorColumn,
  useCoverColors,
  useSettledSort,
  type CoverRef,
  type Rgb,
} from "./coverColor";
import { applySort, type Sort } from "./sort";

vi.mock("./api", () => ({ coverColors: vi.fn() }));

type Key = "color" | "title";
const COLOR: Sort<Key> = { key: "color", dir: "desc" };
const TITLE: Sort<Key> = { key: "title", dir: "asc" };

const shades: Record<string, Rgb> = {
  black: [10, 10, 10],
  red: [200, 30, 30],
  blue: [30, 60, 200],
  yellow: [230, 200, 40],
};
/** Covers are named `<test>-<shade>`, as read colours outlive each test. */
const read = (key: string) => shades[key.split("-")[1]];
const refs = (keys: string[]): CoverRef[] =>
  keys.map((key) => ({ key, path: key }));

describe("switching to the colour sort", () => {
  let root: Root;
  let orders: string[][];
  let finish: () => void;

  /** A shelf worked out the way the Library and Kobo views do. */
  const Shelf = ({ sort, books }: { sort: Sort<Key>; books: string[] }) => {
    const covers = useMemo(() => refs(books), [books]);
    const { colorOf, loading } = useCoverColors(sort?.key === "color", covers);
    const shown = useSettledSort(sort, loading);
    const order = applySort(books, shown, [
      colorColumn<string, Key>("color", colorOf),
      { key: "title", label: "Title", width: "0", value: (b) => b },
    ]);
    if (orders.at(-1)?.join() !== order.join()) orders.push(order);
    return null;
  };
  const show = (sort: Sort<Key>, books: string[]) =>
    act(async () => root.render(<Shelf sort={sort} books={books} />));

  beforeEach(() => {
    (
      globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    orders = [];
    vi.mocked(coverColors).mockReset();
    vi.mocked(coverColors).mockImplementation(
      (paths) =>
        new Promise((resolve) => {
          finish = () => resolve(paths.map(read));
        }),
    );
    root = createRoot(document.createElement("div"));
  });

  afterEach(() => act(async () => root.unmount()));

  it("moves books once when some covers are new", async () => {
    await show(COLOR, ["new-blue"]);
    await act(async () => finish());
    orders = [];
    const books = ["new-black", "new-red", "new-blue"];
    await show(null, books);
    await show(COLOR, books);
    expect(orders).toEqual([books]);
    await act(async () => finish());
    expect(orders).toEqual([books, ["new-red", "new-blue", "new-black"]]);
  });

  it("goes straight from the previous sort to colour", async () => {
    const books = ["from-yellow", "from-black", "from-red"];
    await show(TITLE, books);
    await show(COLOR, books);
    await act(async () => finish());
    expect(orders).toEqual([
      ["from-black", "from-red", "from-yellow"],
      ["from-red", "from-yellow", "from-black"],
    ]);
  });

  it("sorts at once when the colours are already known", async () => {
    const books = ["known-black", "known-red"];
    await show(COLOR, books);
    await act(async () => finish());
    await show(TITLE, books);
    expect(orders.at(-1)).toEqual(books);
    await show(COLOR, books);
    expect(orders.at(-1)).toEqual(["known-red", "known-black"]);
    expect(coverColors).toHaveBeenCalledTimes(1);
  });
});
