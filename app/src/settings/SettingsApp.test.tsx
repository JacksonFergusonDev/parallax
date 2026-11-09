// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ask } from "@tauri-apps/plugin-dialog";
import { DEFAULT_SETTINGS } from "../reader/backend";
import { deletesNow, formatBytes } from "./BackupsPane";
import { SettingsApp } from "./SettingsApp";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  isTauri: () => false,
  convertFileSrc: (path: string) => path,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn() }));

describe("backup sizes", () => {
  it("reads like Finder", () => {
    expect(formatBytes(0)).toBe("Nothing");
    expect(formatBytes(23_400_000)).toBe("23.4 MB");
    expect(formatBytes(500_000_000)).toBe("500 MB");
    expect(formatBytes(2_000_000_000)).toBe("2 GB");
  });

  it("warns only when a change would delete backups now", () => {
    const current = {
      policy: { keep: "history" as const, max_bytes: 500_000_000 },
      used_bytes: 300_000_000,
    };
    expect(deletesNow(current, { keep: "latest", max_bytes: null })).toMatch(
      /Older backups/,
    );
    expect(
      deletesNow(current, { keep: "history", max_bytes: 250_000_000 }),
    ).toMatch(/300 MB.*250 MB/);
    expect(
      deletesNow(current, { keep: "history", max_bytes: 1_000_000_000 }),
    ).toBeNull();
    expect(
      deletesNow(current, { keep: "history", max_bytes: null }),
    ).toBeNull();
  });
});

describe("Settings window", () => {
  let root: Root;
  let host: HTMLDivElement;
  let stored: Record<string, string | null>;
  let policy: { keep: string; max_bytes: number | null };

  const calls = (command: string) =>
    vi.mocked(invoke).mock.calls.filter(([c]) => c === command);
  const button = (text: string) =>
    [...host.querySelectorAll("button")].find(
      (b) => b.textContent?.trim() === text,
    ) as HTMLButtonElement;
  const labelled = (text: string) =>
    [...host.querySelectorAll("label")]
      .find((l) => l.textContent?.includes(text))!
      .querySelector("input") as HTMLInputElement;
  const select = (text: string) => {
    const label = [...host.querySelectorAll("label")].find(
      (l) => l.textContent?.trim() === text,
    ) as HTMLLabelElement;
    return host.querySelector(`[id="${label.htmlFor}"]`) as HTMLSelectElement;
  };
  const settle = () =>
    act(async () => {
      for (let i = 0; i < 5; i++) await Promise.resolve();
    });
  const click = async (element: HTMLElement) => {
    await act(async () => element.click());
    await settle();
  };
  const change = async (element: HTMLSelectElement, value: string) => {
    await act(async () => {
      element.value = value;
      element.dispatchEvent(new Event("change", { bubbles: true }));
    });
    await settle();
  };
  const tab = (name: string) =>
    click(
      [...host.querySelectorAll('[role="tab"]')].find((t) =>
        t.textContent?.includes(name),
      ) as HTMLElement,
    );

  beforeEach(async () => {
    (
      globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
    ).IS_REACT_ACT_ENVIRONMENT = true;
    window.matchMedia = vi.fn(() => ({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })) as unknown as typeof window.matchMedia;
    localStorage.clear();
    stored = {};
    policy = { keep: "history", max_bytes: 500_000_000 };
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      switch (command) {
        case "settings_all":
          return { ...stored };
        case "reader_settings_get":
          return { ...DEFAULT_SETTINGS };
        case "setting_set":
          stored[a.key as string] = a.value as string | null;
          return null;
        case "backup_settings":
          return { policy, used_bytes: 23_400_000 };
        case "backup_settings_set":
          policy = a.policy as typeof policy;
          return { policy, used_bytes: 2_000_000 };
        case "reader_settings_set":
        case "settings_reset":
          return null;
        default:
          throw new Error(`unexpected ${command}`);
      }
    });
    host = document.body.appendChild(document.createElement("div"));
    root = createRoot(host);
    await act(async () => root.render(<SettingsApp />));
    await settle();
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.clearAllMocks();
    delete document.documentElement.dataset.theme;
  });

  it("changes the theme at once, and System clears it", async () => {
    expect(document.documentElement.dataset.theme).toBe("light");
    await click(button("Sepia"));
    expect(calls("setting_set").at(-1)?.[1]).toEqual({
      key: "theme",
      value: "sepia",
    });
    expect(document.documentElement.dataset.theme).toBe("sepia");
    await click(button("System"));
    expect(calls("setting_set").at(-1)?.[1]).toEqual({
      key: "theme",
      value: null,
    });
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("saves Kobo choices as their non-default values", async () => {
    await tab("Kobo");
    const rules = labelled("Apply grouping rules automatically");
    expect(rules.checked).toBe(true);
    await click(rules);
    expect(calls("setting_set").at(-1)?.[1]).toEqual({
      key: "kobo_rules",
      value: "off",
    });
    const eject = labelled("Eject the Kobo");
    expect(eject.checked).toBe(false);
    await click(eject);
    expect(calls("setting_set").at(-1)?.[1]).toEqual({
      key: "eject_after_send",
      value: "on",
    });
  });

  it("tunes comic pages and resets them to recommended", async () => {
    await tab("Comics");
    const reset = button("Using recommended settings");
    expect(reset.disabled).toBe(true);
    const separation = select("Colour separation");
    expect(separation.value).toBe("");
    await change(separation, "150");
    expect(calls("setting_set").at(-1)?.[1]).toEqual({
      key: "comic_colour_separation",
      value: "150",
    });
    await click(labelled("Dither greys"));
    expect(calls("setting_set").at(-1)?.[1]).toEqual({
      key: "comic_dither",
      value: "off",
    });

    const before = calls("setting_set").length;
    await click(button("Reset to recommended"));
    const cleared = calls("setting_set")
      .slice(before)
      .map((call) => call[1]);
    expect(cleared).toEqual([
      { key: "comic_dither", value: null },
      { key: "comic_colour_separation", value: null },
    ]);
    expect(button("Using recommended settings").disabled).toBe(true);
    expect(separation.value).toBe("");
    expect(labelled("Dither greys").checked).toBe(true);
  });

  it("asks before keeping only the latest backup", async () => {
    await tab("Backups");
    expect(host.textContent).toContain("23.4 MB of 500 MB");
    vi.mocked(ask).mockResolvedValueOnce(false);
    await click(labelled("Only the latest"));
    expect(ask).toHaveBeenCalledOnce();
    expect(calls("backup_settings_set")).toHaveLength(0);

    vi.mocked(ask).mockResolvedValueOnce(true);
    await click(labelled("Only the latest"));
    expect(calls("backup_settings_set")[0][1]).toEqual({
      policy: { keep: "latest", max_bytes: 500_000_000 },
    });
    expect(host.textContent).toContain("2 MB of 500 MB");
  });

  it("changes the reader's appearance", async () => {
    await tab("Reader");
    await click(host.querySelector('[aria-label="Larger text"]')!);
    const saved = calls("reader_settings_set").at(-1)?.[1] as {
      settings: typeof DEFAULT_SETTINGS;
    };
    expect(saved.settings).toEqual({ ...DEFAULT_SETTINGS, fontSize: 110 });
    await click(button("Contrast"));
    expect(
      (calls("reader_settings_set").at(-1)?.[1] as typeof saved).settings.theme,
    ).toBe("contrast");
  });

  it("resets only after confirmation", async () => {
    await tab("Advanced");
    vi.mocked(ask).mockResolvedValueOnce(false);
    await click(button("Reset all settings…"));
    expect(calls("settings_reset")).toHaveLength(0);
    vi.mocked(ask).mockResolvedValueOnce(true);
    await click(button("Reset all settings…"));
    expect(calls("settings_reset")).toHaveLength(1);
  });

  it("remembers the open tab", async () => {
    await tab("Library");
    expect(localStorage.getItem("settings-tab")).toBe("library");
    expect(host.textContent).toContain("Review books when added");
  });
});
