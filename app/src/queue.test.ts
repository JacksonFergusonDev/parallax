import { describe, expect, it } from "vitest";
import { queueReducer, sendable, type QueueItem } from "./queue";

const add = (items: QueueItem[], ...paths: string[]) =>
  queueReducer(items, { type: "add", paths });

describe("queueReducer", () => {
  it("adds books once and ignores other files", () => {
    const items = add(
      [],
      "/a/Dune.epub",
      "/a/notes.txt",
      "/a/Dune.epub",
      "/a/Emma.KEPUB",
    );
    expect(items.map((i) => i.name)).toEqual(["Dune.epub", "Emma.KEPUB"]);
    expect(add(items, "/a/Dune.epub")).toHaveLength(2);
  });

  it("maps progress events to the right item by batch index", () => {
    let items = add([], "/a/One.epub", "/a/Two.epub", "/a/Three.epub");
    const batch = ["/a/One.epub", "/a/Three.epub"];
    items = queueReducer(items, {
      type: "progress",
      batch,
      progress: { stage: "converting", index: 2, total: 2, file: "Three.epub" },
    });
    expect(items.map((i) => i.status)).toEqual([
      "queued",
      "queued",
      "converting",
    ]);
    items = queueReducer(items, {
      type: "progress",
      batch,
      progress: {
        stage: "failed",
        index: 2,
        total: 2,
        file: "Three.epub",
        error: "bad zip",
      },
    });
    expect(items[2]).toMatchObject({ status: "failed", error: "bad zip" });
    items = queueReducer(items, {
      type: "progress",
      batch,
      progress: {
        stage: "done",
        index: 1,
        total: 2,
        file: "One.epub",
        device_path: "KEpub/One.kepub.epub",
      },
    });
    expect(items[0]).toMatchObject({
      status: "sent",
      devicePath: "KEpub/One.kepub.epub",
    });
  });

  it("offers queued and failed items for sending, and clears sent ones", () => {
    let items = add([], "/a/One.epub", "/a/Two.epub", "/a/Three.epub");
    items = queueReducer(items, {
      type: "progress",
      batch: ["/a/One.epub"],
      progress: {
        stage: "done",
        index: 1,
        total: 1,
        file: "One.epub",
        device_path: "KEpub/One.kepub.epub",
      },
    });
    expect(sendable(items).map((i) => i.name)).toEqual([
      "Two.epub",
      "Three.epub",
    ]);
    expect(queueReducer(items, { type: "clearFinished" })).toHaveLength(2);
  });
});
