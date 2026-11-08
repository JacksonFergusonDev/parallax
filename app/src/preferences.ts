// The main window's part in Settings: its theme, kept current as Settings
// changes it, and which view it opens on.
import { useEffect, useRef, useState } from "react";
import { getSetting, onSettingChange, setSetting } from "./api";
import { appTheme, followTheme } from "./design/theme";

export type View = "library" | "device" | "reading";

const VIEWS: View[] = ["library", "device", "reading"];

/** The view to open on, from `open_view` and, for `last`, `last_view`. */
export function openingView(
  open: string | null,
  last: string | null,
): View | null {
  if (open === "reading") return "reading";
  if (open === "last" && VIEWS.includes(last as View)) return last as View;
  return null;
}

/** Saves the view the window shows, for opening where the user left off. */
export const rememberView = (view: View) =>
  void setSetting("last_view", view === "library" ? null : view).catch(
    console.error,
  );

/**
 * Applies the saved theme and follows changes to it, and calls `onView` with
 * where Settings says to open if not the library. True once the theme is on
 * the page, so the window never shows in the wrong one.
 */
export function usePreferences(onView: (view: View) => void): boolean {
  const [ready, setReady] = useState(false);
  const opened = useRef(onView);

  useEffect(() => {
    let alive = true;
    const theme = followTheme("system");
    const unlisten = onSettingChange(({ key, value }) => {
      if (key === "theme") theme.set(appTheme(value));
    });
    Promise.all([
      getSetting("theme"),
      getSetting("open_view"),
      getSetting("last_view"),
    ])
      .then(([saved, open, last]) => {
        if (!alive) return;
        theme.set(appTheme(saved));
        const view = openingView(open, last);
        if (view) opened.current(view);
      })
      // The system theme and the library are fine defaults.
      .catch(console.error)
      .finally(() => alive && setReady(true));
    return () => {
      alive = false;
      theme.stop();
      void unlisten.then((f) => f());
    };
  }, []);

  return ready;
}
