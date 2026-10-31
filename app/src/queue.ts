import type { Progress } from "./api";

export type ItemStatus =
  "queued" | "converting" | "copying" | "sent" | "failed";

export interface QueueItem {
  path: string;
  name: string;
  status: ItemStatus;
  error?: string;
  devicePath?: string;
}

export type QueueAction =
  | { type: "add"; paths: string[] }
  | { type: "remove"; path: string }
  | { type: "clearFinished" }
  /** `batch` is the list of paths passed to send, so `progress.index` (1-based) maps into it. */
  | { type: "progress"; batch: string[]; progress: Progress };

const BOOK_EXTENSIONS = [".epub", ".kepub"];

export const isBook = (path: string) =>
  BOOK_EXTENSIONS.some((ext) => path.toLowerCase().endsWith(ext));

const fileName = (path: string) => path.split(/[/\\]/).pop() ?? path;

export function queueReducer(
  items: QueueItem[],
  action: QueueAction,
): QueueItem[] {
  switch (action.type) {
    case "add": {
      const known = new Set(items.map((i) => i.path));
      const added = action.paths
        .filter((p) => isBook(p) && !known.has(p))
        .filter((p, i, all) => all.indexOf(p) === i)
        .map((path): QueueItem => ({
          path,
          name: fileName(path),
          status: "queued",
        }));
      return [...items, ...added];
    }
    case "remove":
      return items.filter((i) => i.path !== action.path);
    case "clearFinished":
      return items.filter((i) => i.status !== "sent");
    case "progress": {
      const { progress, batch } = action;
      const path = batch[progress.index - 1];
      return items.map((item) =>
        item.path === path ? applyProgress(item, progress) : item,
      );
    }
  }
}

function applyProgress(item: QueueItem, p: Progress): QueueItem {
  switch (p.stage) {
    case "converting":
    case "copying":
      return { ...item, status: p.stage, error: undefined };
    case "done":
      return { ...item, status: "sent", devicePath: p.device_path };
    case "failed":
      return { ...item, status: "failed", error: p.error };
  }
}

/** Items a Send click would include: anything not already on the device. */
export const sendable = (items: QueueItem[]) =>
  items.filter((i) => i.status === "queued" || i.status === "failed");
