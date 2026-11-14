import { describe, expect, it } from "vitest";
import { NO_FILTER } from "./libraryFilter";
import {
  activeSaved,
  parseSaved,
  serializeSaved,
  withSaved,
} from "./savedSearches";

describe("saved searches", () => {
  const unread = { ...NO_FILTER, reading: "unread" as const };

  it("round-trip and skip what can't be read", () => {
    const list = withSaved([], {
      name: " To read ",
      query: "",
      filter: unread,
    });
    expect(parseSaved(serializeSaved(list))).toEqual([
      { name: "To read", query: "", filter: unread },
    ]);
    expect(parseSaved("not json")).toEqual([]);
    expect(
      parseSaved('[{"name":""},{"name":"Sci-fi","query":"scifi"}]'),
    ).toEqual([{ name: "Sci-fi", query: "scifi", filter: NO_FILTER }]);
    expect(serializeSaved([])).toBeNull();
  });

  it("replace one of the same name and keep names in order", () => {
    let list = withSaved([], { name: "b", query: "x", filter: NO_FILTER });
    list = withSaved(list, { name: "a", query: "y", filter: NO_FILTER });
    list = withSaved(list, { name: "B", query: "z", filter: NO_FILTER });
    expect(list.map((s) => [s.name, s.query])).toEqual([
      ["a", "y"],
      ["B", "z"],
    ]);
  });

  it("know which one is showing", () => {
    const list = withSaved([], {
      name: "Unread",
      query: "le guin",
      filter: unread,
    });
    expect(activeSaved(list, " le guin ", unread)?.name).toBe("Unread");
    expect(activeSaved(list, "le guin", NO_FILTER)).toBeNull();
  });
});
