// First-run setup: whether it shows, and the steps it walks through. The
// `setup` setting is `started` once it has shown, then `skipped` or `done`.
import { useCallback, useEffect, useState } from "react";
import {
  getSetting,
  setSetting,
  type KoboHealth,
  type SettingKey,
} from "../api";

export type SetupOutcome = "skipped" | "done";

/** What to do at launch: show setup, quietly mark it done, or neither. */
export function setupOnLaunch(
  saved: string | null,
  libraryBooks: number,
): "show" | "adopt" | "hide" {
  if (saved === "done" || saved === "skipped") return "hide";
  // A library that existed before setup did needs no welcome. One that
  // grew during an unfinished setup (`started`) still gets the rest of it.
  if (saved === null && libraryBooks > 0) return "adopt";
  return "show";
}

export const STEPS = [
  "welcome",
  "connect",
  "books",
  "settings",
  "appearance",
  "tour",
  "done",
] as const;
export type Step = (typeof STEPS)[number];

/** The steps the progress dots count: all but the first and last. */
export const COUNTED: readonly Step[] = STEPS.slice(1, -1);

export const nextStep = (step: Step): Step =>
  STEPS[Math.min(STEPS.indexOf(step) + 1, STEPS.length - 1)];
export const previousStep = (step: Step): Step =>
  STEPS[Math.max(STEPS.indexOf(step) - 1, 0)];

/** The settings setup offers. Unset is the recommended value of each. */
export const SETUP_SETTINGS = [
  "kobo_series",
  "kobo_rules",
  "eject_after_send",
  "review_on_add",
  "comics_kind",
  "open_view",
  "reader_windows",
] as const satisfies readonly SettingKey[];

type Values = Partial<Record<SettingKey, string | null>>;

/** Those of `SETUP_SETTINGS` changed from what's recommended. */
export const customised = (values: Values) =>
  SETUP_SETTINGS.filter((key) => values[key] != null);

/** Things worth tidying on a Kobo. Books it hasn't imported yet are normal. */
export function healthIssues(health: KoboHealth): number {
  return (
    health.missing_files.length +
    health.duplicate_collections.length +
    (health.orphaned_images > 0 ? 1 : 0)
  );
}

/**
 * Decides at launch whether setup shows, once the library has loaded. `known`
 * is false until then, so the window never flashes the library first.
 */
export function useSetup(loaded: boolean, libraryBooks: number) {
  const [open, setOpen] = useState(false);
  const [known, setKnown] = useState(false);

  useEffect(() => {
    if (!loaded || known) return;
    let alive = true;
    getSetting("setup")
      .then((saved) => {
        if (!alive) return;
        const decision = setupOnLaunch(saved, libraryBooks);
        if (decision === "adopt") return setSetting("setup", "done");
        if (decision === "show") {
          setOpen(true);
          if (saved === null) return setSetting("setup", "started");
        }
      })
      // Setup is a convenience; the library is a fine place to start.
      .catch(console.error)
      .finally(() => alive && setKnown(true));
    return () => {
      alive = false;
    };
  }, [loaded, known, libraryBooks]);

  const show = useCallback(() => setOpen(true), []);
  const finish = useCallback((outcome: SetupOutcome) => {
    setOpen(false);
    void setSetting("setup", outcome).catch(console.error);
  }, []);

  return { open, known, show, finish };
}
