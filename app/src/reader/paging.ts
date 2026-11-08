/** How far to scroll before a page turns. */
const TURN_AFTER = 40;
/** A trackpad keeps sending events as it coasts; this quiet ends the gesture. */
const GESTURE_GAP_MS = 160;

export interface Wheel {
  deltaX: number;
  deltaY: number;
  ctrlKey: boolean;
  timeStamp: number;
}

/** Turns scrolling into page turns: one for each gesture, however long. */
export function wheelPager(turn: (direction: 1 | -1) => void) {
  let total = 0;
  let turned = false;
  let last = -Infinity;
  return (event: Wheel) => {
    if (event.ctrlKey) return; // pinching
    if (event.timeStamp - last > GESTURE_GAP_MS) {
      total = 0;
      turned = false;
    }
    last = event.timeStamp;
    if (turned) return;
    const horizontal = Math.abs(event.deltaX) > Math.abs(event.deltaY);
    total += horizontal ? event.deltaX : event.deltaY;
    if (Math.abs(total) >= TURN_AFTER) {
      turned = true;
      turn(total > 0 ? 1 : -1);
    }
  };
}

export type Key =
  | "left"
  | "right"
  | "next"
  | "previous"
  | "larger"
  | "smaller"
  | "close"
  | "find"
  | "findNext"
  | "findPrevious";

interface KeyPress {
  key: string;
  /** Typing in a field: only shortcuts and Escape apply there. */
  typing?: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
}

export function keyAction(event: KeyPress): Key | null {
  if (event.ctrlKey || event.altKey) return null;
  if (event.metaKey) {
    if (event.key === "=" || event.key === "+") return "larger";
    if (event.key === "-") return "smaller";
    if (event.key === "f") return "find";
    if (event.key === "g") return event.shiftKey ? "findPrevious" : "findNext";
    return null;
  }
  if (event.typing) return event.key === "Escape" ? "close" : null;
  switch (event.key) {
    case "ArrowLeft":
      return "left";
    case "ArrowRight":
      return "right";
    case "ArrowDown":
    case "PageDown":
      return "next";
    case "ArrowUp":
    case "PageUp":
      return "previous";
    case " ":
      return event.shiftKey ? "previous" : "next";
    case "Escape":
      return "close";
    default:
      return null;
  }
}

/** Very short books can report more than 1. */
export const percent = (fraction: number) =>
  `${Math.round(Math.min(Math.max(fraction, 0), 1) * 100)}%`;

/** Rejects if `work` hasn't finished, as the engine very rarely hangs. */
export function within<T>(work: Promise<T>, ms: number): Promise<T> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("timed out")), ms);
    work.then(resolve, reject).finally(() => clearTimeout(timer));
  });
}
