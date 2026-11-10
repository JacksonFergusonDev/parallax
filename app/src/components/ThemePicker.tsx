// The appearance picker shared by Settings and first-run setup.
import { APP_THEMES, type AppTheme } from "../design/theme";
import "./themePicker.css";

/** Each theme as a small picture of itself. */
export function ThemePicker({
  value,
  onChange,
}: {
  value: AppTheme;
  onChange: (theme: AppTheme) => void;
}) {
  const sample = (theme: string) => (
    <span className="theme-sample" data-theme={theme}>
      <span className="theme-sample-bar" />
      <span className="theme-sample-text" />
      <span className="theme-sample-text short" />
      <span className="theme-sample-accent" />
    </span>
  );
  return (
    <div className="theme-picker" role="radiogroup" aria-label="Appearance">
      {APP_THEMES.map((theme) => (
        <button
          key={theme.value}
          role="radio"
          aria-checked={value === theme.value}
          className="theme-option"
          onClick={() => onChange(theme.value)}
        >
          <span className="theme-preview" aria-hidden="true">
            {theme.value === "system" ? (
              <>
                {sample("light")}
                {sample("dark")}
              </>
            ) : theme.value === "contrast" ? (
              <>
                {sample("contrast")}
                {sample("contrast-dark")}
              </>
            ) : (
              sample(theme.value)
            )}
          </span>
          <span>{theme.label}</span>
        </button>
      ))}
    </div>
  );
}
