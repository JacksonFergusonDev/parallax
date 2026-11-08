// The main window's side of the menu bar (src-tauri/src/menu.rs). Components
// register the commands they can run; the menu shows which are available,
// with checkmarks, and a click runs the same code as the matching button.
import { useEffect, useRef } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Sort } from "./sort";

export interface MenuCommand {
  run: () => void;
  /** Defaults to true. */
  enabled?: boolean;
  /** For checkmark items. */
  checked?: boolean;
}

export type MenuCommands = Record<string, MenuCommand>;

interface Entry {
  owner: symbol;
  command: MenuCommand;
}

const registry = new Map<string, Entry>();
let reported = "";
let scheduled = false;

/** What the menu should show for every registered command. */
export function menuState(commands: Map<string, Entry> = registry) {
  return Object.fromEntries(
    [...commands].map(([id, { command }]) => [
      id,
      {
        enabled: command.enabled ?? true,
        ...(command.checked === undefined ? {} : { checked: command.checked }),
      },
    ]),
  );
}

/** Tells the menu, once per batch of changes and only when something changed. */
function report() {
  if (scheduled) return;
  scheduled = true;
  queueMicrotask(() => {
    scheduled = false;
    const items = menuState();
    const json = JSON.stringify(items);
    if (json === reported) return;
    reported = json;
    if (isTauri()) void invoke("menu_state", { items }).catch(console.error);
  });
}

/** Runs the command the menu sent, if it's registered and available. */
export function runMenuCommand(id: string) {
  const entry = registry.get(id);
  if (entry && entry.command.enabled !== false) entry.command.run();
}

let listening = false;
function listenOnce() {
  if (listening || !isTauri()) return;
  listening = true;
  void listen<string>("menu-command", (event) => runMenuCommand(event.payload));
}

/**
 * Registers `commands` for as long as the component is mounted, keeping
 * each one current: pass a new object every render.
 */
export function useMenuCommands(commands: MenuCommands) {
  const owner = useRef(Symbol("menu commands"));
  const mine = useRef<string[]>([]);

  useEffect(() => {
    listenOnce();
    const me = owner.current;
    for (const id of mine.current)
      if (!(id in commands) && registry.get(id)?.owner === me)
        registry.delete(id);
    for (const [id, command] of Object.entries(commands))
      registry.set(id, { owner: me, command });
    mine.current = Object.keys(commands);
    report();
  });

  useEffect(() => {
    const me = owner.current;
    return () => {
      for (const id of mine.current)
        if (registry.get(id)?.owner === me) registry.delete(id);
      report();
    };
  }, []);
}

/** Sort By for a view with these sort `keys`, and its order. */
export function sortCommands<K extends string>(
  sort: Sort<K>,
  setSort: (sort: Sort<K>) => void,
  keys: readonly K[],
): MenuCommands {
  const commands: MenuCommands = {
    "sort.default": { run: () => setSort(null), checked: sort === null },
    "sort.desc": {
      run: () => sort && setSort({ ...sort, dir: "desc" }),
      enabled: sort !== null,
      checked: sort?.dir === "desc",
    },
    "sort.asc": {
      run: () => sort && setSort({ ...sort, dir: "asc" }),
      enabled: sort !== null,
      checked: sort?.dir === "asc",
    },
  };
  for (const key of keys)
    commands[`sort.${key}`] = {
      run: () => setSort({ key, dir: sort?.dir ?? "desc" }),
      checked: sort?.key === key,
    };
  return commands;
}

/** For tests: forgets every registration. */
export function resetMenu() {
  registry.clear();
  reported = "";
}
