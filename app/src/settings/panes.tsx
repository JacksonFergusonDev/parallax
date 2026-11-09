import { useId, useState, type ReactNode } from "react";
import { ask } from "@tauri-apps/plugin-dialog";
import {
  COMIC_PAGE_SETTINGS,
  resetSettings,
  showFolder,
  type SettingKey,
} from "../api";
import { APP_THEMES, appTheme, type AppTheme } from "../design/theme";
import type { Values } from "./useSettings";

export interface PaneProps {
  values: Values;
  set: (key: SettingKey, value: string | null) => void;
}

/** A labelled row: the label on the left, its controls and hint beside it. */
export function Row({
  label,
  hint,
  children,
  labelFor,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
  /** The control the label names; otherwise the row is a labelled group. */
  labelFor?: string;
}) {
  const id = useId();
  return (
    <div
      className="settings-row"
      role={labelFor ? undefined : "group"}
      aria-labelledby={labelFor ? undefined : id}
    >
      {labelFor ? (
        <label className="settings-label" htmlFor={labelFor}>
          {label}
        </label>
      ) : (
        <span className="settings-label" id={id}>
          {label}
        </span>
      )}
      <div className="settings-control">
        {children}
        {hint && <p className="hint">{hint}</p>}
      </div>
    </div>
  );
}

/** A checkbox for a setting whose default is on (`off` turns it off) or,
 * with `defaultOn` false, off (`on` turns it on). */
export function Check({
  values,
  set,
  name,
  defaultOn = true,
  children,
  hint,
}: PaneProps & {
  name: SettingKey;
  defaultOn?: boolean;
  children: ReactNode;
  hint?: ReactNode;
}) {
  const on = defaultOn ? values[name] !== "off" : values[name] === "on";
  return (
    <div className="settings-check">
      <label>
        <input
          type="checkbox"
          checked={on}
          onChange={(e) => {
            const next = e.target.checked;
            set(name, next === defaultOn ? null : next ? "on" : "off");
          }}
        />
        {children}
      </label>
      {hint && <p className="hint">{hint}</p>}
    </div>
  );
}

/** Radio buttons, each with an optional hint. */
export function Choices<T extends string>({
  name,
  value,
  options,
  onChange,
}: {
  name: string;
  value: T;
  options: { value: T; label: string; hint?: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div className="settings-choices">
      {options.map((option) => (
        <div className="settings-check" key={option.value}>
          <label>
            <input
              type="radio"
              name={name}
              checked={value === option.value}
              onChange={() => onChange(option.value)}
            />
            {option.label}
          </label>
          {option.hint && <p className="hint">{option.hint}</p>}
        </div>
      ))}
    </div>
  );
}

/** Each theme as a small picture of itself. */
function ThemePicker({
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

export function GeneralPane({ values, set }: PaneProps) {
  const openId = useId();
  const theme = appTheme(values.theme);
  return (
    <>
      <Row
        label="Appearance"
        hint={
          theme === "system" || theme === "contrast"
            ? "Light or dark as macOS is set."
            : undefined
        }
      >
        <ThemePicker
          value={theme}
          onChange={(next) => set("theme", next === "system" ? null : next)}
        />
      </Row>
      <Row label="Open to" labelFor={openId}>
        <select
          id={openId}
          value={values.open_view ?? "library"}
          onChange={(e) =>
            set(
              "open_view",
              e.target.value === "library" ? null : e.target.value,
            )
          }
        >
          <option value="library">Library</option>
          <option value="reading">Reading</option>
          <option value="last">Where you left off</option>
        </select>
      </Row>
      <Row label="Reading books">
        <Choices
          name="reader-windows"
          value={values.reader_windows === "one" ? "one" : "each"}
          options={[
            { value: "each", label: "Open each book in its own window" },
            {
              value: "one",
              label: "Read in one window",
              hint: "Opening a book closes the one you were reading. Your place is kept.",
            },
          ]}
          onChange={(next) =>
            set("reader_windows", next === "one" ? "one" : null)
          }
        />
      </Row>
    </>
  );
}

/** Credentials are entered in a password field and replaced as a whole. */
function Credential({
  values,
  set,
  setting,
  label,
  placeholder,
}: PaneProps & {
  setting: "google_books_key" | "metron_token";
  label: string;
  placeholder: string;
}) {
  const saved = !!values[setting];
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const id = useId();
  if (saved && !editing)
    return (
      <div className="settings-inline">
        <span>Saved</span>
        <button onClick={() => setEditing(true)}>Change…</button>
        <button className="danger" onClick={() => set(setting, null)}>
          Remove
        </button>
      </div>
    );
  return (
    <form
      className="settings-inline"
      onSubmit={(e) => {
        e.preventDefault();
        if (!draft.trim()) return;
        set(setting, draft);
        setDraft("");
        setEditing(false);
      }}
    >
      <input
        id={id}
        type="password"
        aria-label={label}
        placeholder={placeholder}
        autoComplete="off"
        spellCheck={false}
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
      />
      <button type="submit" disabled={!draft.trim()}>
        Save
      </button>
      {editing && (
        <button type="button" onClick={() => setEditing(false)}>
          Cancel
        </button>
      )}
    </form>
  );
}

export function LibraryPane(props: PaneProps) {
  return (
    <>
      <Row label="New books">
        <Check
          {...props}
          name="review_on_add"
          hint="Points out missing covers, series and duplicates after books are added."
        >
          Review books when added
        </Check>
      </Row>
      <Row
        label="Google Books"
        hint="Optional. A free API key lets Look up details find genres for more books."
      >
        <Credential
          {...props}
          setting="google_books_key"
          label="Google Books API key"
          placeholder="API key"
        />
      </Row>
      <Row label="Library folder">
        <div className="settings-inline">
          <button onClick={() => void showFolder("library")}>
            Show in Finder
          </button>
        </div>
      </Row>
    </>
  );
}

export function KoboPane(props: PaneProps) {
  return (
    <>
      <Row label="When a Kobo connects">
        <Check
          {...props}
          name="kobo_series"
          hint="The Kobo ignores series in books copied to it, so Parallax adds them once it has imported them."
        >
          Add series to newly imported books
        </Check>
      </Row>
      <Row label="Collections">
        <Check
          {...props}
          name="kobo_rules"
          hint="After sending books and when a Kobo connects, your grouping rules file books into collections the Kobo already has. New collections are made only when you apply rules yourself."
        >
          Apply grouping rules automatically
        </Check>
      </Row>
      <Row label="After sending">
        <Check
          {...props}
          name="eject_after_send"
          defaultOn={false}
          hint="So the Kobo is ready to unplug once every book has been sent."
        >
          Eject the Kobo
        </Check>
      </Row>
    </>
  );
}

export function AdvancedPane({
  onError,
}: {
  onError: (message: string) => void;
}) {
  const [resetting, setResetting] = useState(false);
  const reset = async () => {
    const sure = await ask(
      "Every setting goes back to its default, including the reader's appearance and how backups are kept. Your Google Books key stays.",
      {
        title: "Reset all settings?",
        kind: "warning",
        okLabel: "Reset",
        cancelLabel: "Cancel",
      },
    );
    if (!sure) return;
    setResetting(true);
    try {
      await resetSettings();
    } catch (e) {
      onError(String(e));
    } finally {
      setResetting(false);
    }
  };
  return (
    <>
      <Row label="App data" hint="Your library, settings and Kobo backups.">
        <div className="settings-inline">
          <button onClick={() => void showFolder("data")}>
            Show in Finder
          </button>
        </div>
      </Row>
      <Row label="Reset">
        <div className="settings-inline">
          <button onClick={() => void reset()} disabled={resetting}>
            {resetting ? "Resetting…" : "Reset all settings…"}
          </button>
        </div>
      </Row>
    </>
  );
}

/** Strength presets, as percentages of the recommended strength. */
const STRENGTHS: [string | null, string][] = [
  ["0", "Off"],
  ["50", "50%"],
  ["75", "75%"],
  [null, "100% (recommended)"],
  ["125", "125%"],
  ["150", "150%"],
  ["200", "200%"],
];

function Strength({
  values,
  set,
  name,
  id,
  hint,
}: PaneProps & { name: SettingKey; id: string; hint: string }) {
  return (
    <select
      id={id}
      aria-describedby={hint}
      value={values[name] ?? ""}
      onChange={(e) => set(name, e.target.value || null)}
    >
      {STRENGTHS.map(([value, label]) => (
        <option key={label} value={value ?? ""}>
          {label}
        </option>
      ))}
    </select>
  );
}

/** How comics come into the library and how their pages are prepared for
 * the Kobo. Every page setting has a recommended default chosen on a Libra
 * 2, and one button puts them all back. */
export function ComicsPane(props: PaneProps) {
  const kind = useId();
  const spreads = useId();
  const separation = useId();
  const separationHint = useId();
  const darkening = useId();
  const darkeningHint = useId();
  const changed = COMIC_PAGE_SETTINGS.some((key) => props.values[key] != null);
  return (
    <>
      <Row
        label="Import comics as"
        labelFor={kind}
        hint="Used when ComicInfo has no reading kind."
      >
        <select
          id={kind}
          value={props.values.comics_kind ?? "manga"}
          onChange={(e) =>
            props.set(
              "comics_kind",
              e.target.value === "comic" ? "comic" : null,
            )
          }
        >
          <option value="manga">Manga</option>
          <option value="comic">Comics</option>
        </select>
      </Row>
      <Row
        label="Metron API token"
        hint="Create a token in your Metron account settings to look up Western comic issues. Manga lookups use Mangainfo and need no account."
      >
        <Credential
          {...props}
          setting="metron_token"
          label="Metron API token"
          placeholder="API token"
        />
      </Row>
      <Row
        label="Spreads"
        labelFor={spreads}
        hint="Two-page spreads on the Kobo. Very wide pages are rotated to fit in every mode."
      >
        <select
          id={spreads}
          value={props.values.comic_spreads ?? "split"}
          onChange={(e) =>
            props.set(
              "comic_spreads",
              e.target.value === "split" ? null : e.target.value,
            )
          }
        >
          <option value="split">Split (recommended)</option>
          <option value="keep_whole">Keep whole</option>
          <option value="whole_then_halves">Whole then halves</option>
        </select>
      </Row>
      <Row label="All pages">
        <Check {...props} name="comic_crop" defaultOn={false}>
          Trim white margins
        </Check>
        <Check {...props} name="comic_upscale" defaultOn={false}>
          Enlarge small pages
        </Check>
      </Row>
      <Row label="Black-and-white pages">
        <Check
          {...props}
          name="comic_dither"
          hint="Smoother greys in shading and gradients, for slightly larger files."
        >
          Dither greys
        </Check>
        <Check {...props} name="comic_autocontrast" defaultOn={false}>
          Increase contrast
        </Check>
      </Row>
      <Row label="Colour pages">
        <p className="hint settings-lead">
          On black-and-white Kobos. Colour Kobos keep colour pages in colour.
        </p>
        <Check
          {...props}
          name="comic_colour_contrast"
          hint="Lifts the washed-out look colour art has in grey."
        >
          Increase contrast
        </Check>
        <label className="settings-field" htmlFor={separation}>
          Colour separation
        </label>
        <Strength
          {...props}
          name="comic_colour_separation"
          id={separation}
          hint={separationHint}
        />
        <p className="hint" id={separationHint}>
          Colours that would turn the same grey, like red on grey, become
          different greys.
        </p>
        <label className="settings-field" htmlFor={darkening}>
          Darken mid-tones on light pages
        </label>
        <Strength
          {...props}
          name="comic_midtone_darkening"
          id={darkening}
          hint={darkeningHint}
        />
        <p className="hint" id={darkeningHint}>
          Strengthens the darker parts of mostly light art. Dark pages are left
          alone.
        </p>
      </Row>
      <Row
        label="Recommended"
        hint="Comics already on your Kobo are offered for update when their page settings change."
      >
        <div className="settings-inline">
          <button
            disabled={!changed}
            onClick={() => {
              for (const key of COMIC_PAGE_SETTINGS) {
                if (props.values[key] != null) props.set(key, null);
              }
            }}
          >
            {changed ? "Reset to recommended" : "Using recommended settings"}
          </button>
        </div>
      </Row>
    </>
  );
}
