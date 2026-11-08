// The app's theme, shared by every window. tokens.css holds each palette
// under `:root[data-theme=…]`; this decides which one a window shows.

/** What the user picks in Settings. */
export type AppTheme = "system" | "light" | "dark" | "sepia" | "contrast";

/** The palette a window actually shows. */
export type ShownTheme =
  "light" | "dark" | "sepia" | "contrast" | "contrast-dark";

export const APP_THEMES: { value: AppTheme; label: string }[] = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "sepia", label: "Sepia" },
  { value: "contrast", label: "High contrast" },
];

/** The saved setting as a theme; anything unknown is the system's. */
export const appTheme = (value: string | null | undefined): AppTheme =>
  APP_THEMES.some((t) => t.value === value) ? (value as AppTheme) : "system";

/** System and High contrast follow macOS between light and dark. */
export function shownTheme(theme: AppTheme, systemDark: boolean): ShownTheme {
  switch (theme) {
    case "system":
      return systemDark ? "dark" : "light";
    case "contrast":
      return systemDark ? "contrast-dark" : "contrast";
    default:
      return theme;
  }
}

export const isDark = (theme: ShownTheme) =>
  theme === "dark" || theme === "contrast-dark";

const systemDark = () =>
  typeof window.matchMedia === "function" &&
  window.matchMedia("(prefers-color-scheme: dark)").matches;

/** Shows `theme` on this window's page. */
export function showTheme(shown: ShownTheme, root = document.documentElement) {
  root.dataset.theme = shown;
  root.style.colorScheme = isDark(shown) ? "dark" : "light";
}

/**
 * Keeps this window in `theme`, following macOS where the theme does.
 * Returns a function that changes the theme, and one that stops following.
 */
export function followTheme(initial: AppTheme) {
  let theme = initial;
  const apply = () => showTheme(shownTheme(theme, systemDark()));
  const media =
    typeof window.matchMedia === "function"
      ? window.matchMedia("(prefers-color-scheme: dark)")
      : null;
  media?.addEventListener("change", apply);
  apply();
  return {
    set(next: AppTheme) {
      theme = next;
      apply();
    },
    stop: () => media?.removeEventListener("change", apply),
  };
}
