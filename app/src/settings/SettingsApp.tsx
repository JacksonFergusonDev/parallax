import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import {
  Archive,
  BookImage,
  BookOpen,
  Library,
  Settings2,
  Tablet,
  Wrench,
} from "lucide-react";
import { appTheme, followTheme } from "../design/theme";
import { BackupsPane } from "./BackupsPane";
import {
  AdvancedPane,
  ComicsPane,
  GeneralPane,
  KoboPane,
  LibraryPane,
} from "./panes";
import { ReaderPane } from "./ReaderPane";
import { useSettings } from "./useSettings";

const TABS = [
  { id: "general", label: "General", icon: Settings2 },
  { id: "library", label: "Library", icon: Library },
  { id: "kobo", label: "Kobo", icon: Tablet },
  { id: "comics", label: "Comics", icon: BookImage },
  { id: "backups", label: "Backups", icon: Archive },
  { id: "reader", label: "Reader", icon: BookOpen },
  { id: "advanced", label: "Advanced", icon: Wrench },
] as const;
export type Tab = (typeof TABS)[number]["id"];

const TAB_KEY = "settings-tab";

/** The tab last open, a per-window convenience that may be unavailable. */
function rememberedTab(): Tab {
  try {
    const saved = localStorage.getItem(TAB_KEY);
    if (TABS.some((t) => t.id === saved)) return saved as Tab;
  } catch {
    // Storage can be blocked; start on General.
  }
  return "general";
}

export function SettingsApp() {
  const [tab, setTab] = useState<Tab>(rememberedTab);
  const [error, setError] = useState<string | null>(null);
  const onError = useCallback((message: string) => setError(message), []);
  const { values, set, reader, setAppearance } = useSettings(onError);
  const theme = useRef<ReturnType<typeof followTheme> | null>(null);
  const shown = useRef(false);
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);

  const ready = values !== null && reader !== null;
  const themeValue = values?.theme;
  useEffect(() => {
    if (!values) return;
    if (theme.current) theme.current.set(appTheme(themeValue));
    else theme.current = followTheme(appTheme(themeValue));
  }, [values, themeValue]);
  useEffect(() => () => theme.current?.stop(), []);

  // Shown once there's something to show, like the main window.
  useEffect(() => {
    if ((!ready && !error) || shown.current) return;
    shown.current = true;
    if (isTauri()) void invoke("show_window").catch(console.error);
  }, [ready, error]);

  const choose = (next: Tab) => {
    setTab(next);
    try {
      localStorage.setItem(TAB_KEY, next);
    } catch {
      // Not remembered; nothing else depends on it.
    }
  };

  const onTabKey = (event: React.KeyboardEvent, index: number) => {
    const step =
      event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
    if (!step) return;
    event.preventDefault();
    const next = (index + step + TABS.length) % TABS.length;
    choose(TABS[next].id);
    tabs.current[next]?.focus();
  };

  return (
    <div className="settings">
      <nav className="settings-tabs" role="tablist" aria-label="Settings">
        {TABS.map(({ id, label, icon: Icon }, index) => (
          <button
            key={id}
            ref={(el) => {
              tabs.current[index] = el;
            }}
            role="tab"
            id={`tab-${id}`}
            aria-selected={tab === id}
            aria-controls="settings-pane"
            tabIndex={tab === id ? 0 : -1}
            onClick={() => choose(id)}
            onKeyDown={(e) => onTabKey(e, index)}
          >
            <Icon aria-hidden="true" />
            <span>{label}</span>
          </button>
        ))}
      </nav>
      {error && (
        <p className="settings-error" role="alert">
          {error}
          <button className="link" onClick={() => setError(null)}>
            Dismiss
          </button>
        </p>
      )}
      <main
        className="settings-pane"
        id="settings-pane"
        role="tabpanel"
        aria-labelledby={`tab-${tab}`}
      >
        {ready && (
          <>
            {tab === "general" && <GeneralPane values={values} set={set} />}
            {tab === "library" && <LibraryPane values={values} set={set} />}
            {tab === "kobo" && <KoboPane values={values} set={set} />}
            {tab === "comics" && <ComicsPane values={values} set={set} />}
            {tab === "backups" && <BackupsPane onError={onError} />}
            {tab === "reader" && (
              <ReaderPane settings={reader} onChange={setAppearance} />
            )}
            {tab === "advanced" && <AdvancedPane onError={onError} />}
          </>
        )}
      </main>
    </div>
  );
}
