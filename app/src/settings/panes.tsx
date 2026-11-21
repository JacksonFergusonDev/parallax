import { useId, useState, type ReactNode } from "react";
import { ask, open } from "@tauri-apps/plugin-dialog";
import {
  COMIC_PAGE_SETTINGS,
  resetSettings,
  showFolder,
  type SettingKey,
} from "../api";
import { appTheme } from "../design/theme";
import { ThemePicker } from "../components/ThemePicker";
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
  disabled = false,
  children,
  hint,
}: PaneProps & {
  name: SettingKey;
  defaultOn?: boolean;
  disabled?: boolean;
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
          disabled={disabled}
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
      <Row label="Updates">
        <Check
          values={values}
          set={set}
          name="updates_auto"
          hint="Parallax checks when it opens and once a day. Check for Updates… in the Parallax menu checks now."
        >
          Check for updates automatically
        </Check>
        <Check
          values={values}
          set={set}
          name="updates_download"
          defaultOn={false}
          disabled={values.updates_auto === "off"}
          hint="New versions download in the background, then Parallax tells you and installs when you choose to restart. Otherwise it asks before downloading."
        >
          Download updates automatically
        </Check>
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

/** A folder whose new books are added on their own, like calibre's
 * auto-add folder. Off until you choose one. */
function WatchFolder(props: PaneProps) {
  const { values, set } = props;
  const folder = values.watch_folder;
  const choose = async () => {
    const picked = await open({
      directory: true,
      title: "Add books from this folder",
      defaultPath: folder ?? undefined,
    });
    if (typeof picked === "string") set("watch_folder", picked);
  };
  return (
    <Row
      label="Add automatically"
      hint={
        folder
          ? "Books saved here are added to your library while Parallax is open, once they've finished downloading. Subfolders are left alone."
          : "Choose a folder, such as Downloads, and books saved in it are added to your library while Parallax is open."
      }
    >
      {folder && <p className="settings-value settings-path">{folder}</p>}
      <div className="settings-inline">
        <button onClick={() => void choose()}>
          {folder ? "Change folder…" : "Choose folder…"}
        </button>
        {folder && (
          <button onClick={() => set("watch_folder", null)}>
            Stop watching
          </button>
        )}
      </div>
      {folder && (
        <Check
          {...props}
          name="watch_folder_trash"
          defaultOn={false}
          hint="Once a book is in your library, its file in the folder moves to the Trash. Your library keeps its own copy."
        >
          Move added files to the Trash
        </Check>
      )}
    </Row>
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
      <Row label="Searching">
        <Check
          {...props}
          name="library_saved_searches"
          defaultOn={false}
          hint="Keep a search and its filters under a name, such as Unread sci-fi, and bring it back from the filter bar. Turning this off keeps the ones you've saved."
        >
          Show saved searches
        </Check>
      </Row>
      <WatchFolder {...props} />
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

/** What you're typing in a field saved when you leave it, starting again
 * from the saved value whenever that changes, such as from another window. */
function useDraft(saved: string) {
  const [draft, setDraft] = useState(saved);
  const [base, setBase] = useState(saved);
  if (saved !== base) {
    setBase(saved);
    setDraft(saved);
  }
  return [draft, setDraft] as const;
}

/** Multi-line text saved when you leave the field, not on every key. */
function TextSetting({
  values,
  set,
  name,
  id,
  label,
  placeholder,
  describedBy,
}: PaneProps & {
  name: SettingKey;
  id: string;
  label?: string;
  placeholder?: string;
  describedBy?: string;
}) {
  const saved = values[name] ?? "";
  const [draft, setDraft] = useDraft(saved);
  return (
    <textarea
      id={id}
      aria-label={label}
      aria-describedby={describedBy}
      rows={4}
      spellCheck={false}
      autoCapitalize="off"
      autoCorrect="off"
      placeholder={placeholder}
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => {
        if (draft.trim() !== saved.trim()) set(name, draft.trim() || null);
      }}
    />
  );
}

/** How kepubify changes reflowable books on the way to the Kobo. All off
 * by default, so books arrive as their publisher made them. */
function SendingSection(props: PaneProps) {
  const hyphenation = useId();
  const css = useId();
  const cssHint = useId();
  return (
    <>
      <h2 className="settings-section">Sending books</h2>
      <p className="settings-intro">
        These change books you send from now on. To change a book already on
        your Kobo, choose Update on Kobo.
      </p>
      <Row
        label="Hyphenation"
        labelFor={hyphenation}
        hint="Hyphenating long words evens out the spacing in justified text."
      >
        <select
          id={hyphenation}
          value={props.values.kepub_hyphenation ?? "book"}
          onChange={(e) =>
            props.set(
              "kepub_hyphenation",
              e.target.value === "book" ? null : e.target.value,
            )
          }
        >
          <option value="book">As the book sets it</option>
          <option value="on">Always hyphenate</option>
          <option value="off">Never hyphenate</option>
        </select>
      </Row>
      <Row label="Text">
        <Check
          {...props}
          name="kepub_smart_punctuation"
          defaultOn={false}
          hint="Straight quotes and double hyphens become curly quotes and dashes."
        >
          Smarten punctuation
        </Check>
        <Check
          {...props}
          name="kepub_fullscreen_fixes"
          defaultOn={false}
          hint="Works around Kobo firmware bugs that cut off or misplace text when you read in full screen."
        >
          Add full-screen reading fixes
        </Check>
      </Row>
      <Row label="Extra CSS" labelFor={css}>
        <TextSetting
          {...props}
          name="kepub_css"
          id={css}
          describedBy={cssHint}
          placeholder="p { text-indent: 1.5em; }"
        />
        <p className="hint" id={cssHint}>
          Added to every book you send, like calibre&rsquo;s kobo_extra.css.
          Leave it empty to use each book&rsquo;s own styles.
        </p>
      </Row>
    </>
  );
}

/** Fonts every current Kobo has built in. Nickel stores the name as shown. */
const KOBO_FONTS = [
  "Amasis",
  "Avenir Next",
  "Caecilia",
  "Georgia",
  "Gill Sans",
  "Kobo Nickel",
  "Malabar",
  "Optima",
  "Rakuten Sans",
  "Rakuten Serif",
];
/** Matches LINE_HEIGHTS in crates/device-kobo/src/layout.rs. */
const LINE_HEIGHTS = [
  "1.0",
  "1.05",
  "1.07",
  "1.1",
  "1.2",
  "1.35",
  "1.5",
  "1.7",
  "1.8",
  "2.0",
  "2.2",
  "2.4",
  "2.6",
  "2.8",
  "3.0",
];

/** A whole number saved when you leave the field; empty is the default. */
function NumberSetting({
  values,
  set,
  name,
  id,
  min,
  max,
  placeholder,
}: PaneProps & {
  name: SettingKey;
  id: string;
  min: number;
  max: number;
  placeholder: string;
}) {
  const saved = values[name] ?? "";
  const [draft, setDraft] = useDraft(saved);
  const commit = () => {
    const text = draft.trim();
    if (!text) {
      if (saved) set(name, null);
      return;
    }
    const n = Math.round(Number(text));
    if (!Number.isFinite(n)) {
      setDraft(saved);
      return;
    }
    const clamped = String(Math.min(max, Math.max(min, n)));
    setDraft(clamped);
    if (clamped !== saved) set(name, clamped);
  };
  return (
    <input
      id={id}
      type="number"
      inputMode="numeric"
      className="settings-number"
      min={min}
      max={max}
      step={1}
      placeholder={placeholder}
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
      }}
    />
  );
}

/** The font and layout Parallax sets on books on the Kobo, as Kobo
 * Utilities does. Off by default: most people set these on the Kobo. */
function LayoutSection(props: PaneProps) {
  const { values, set } = props;
  const font = useId();
  const size = useId();
  const spacing = useId();
  const margins = useId();
  const alignment = useId();
  const fonts =
    values.kobo_layout_font && !KOBO_FONTS.includes(values.kobo_layout_font)
      ? [...KOBO_FONTS, values.kobo_layout_font]
      : KOBO_FONTS;
  const pick = (key: SettingKey) => (e: React.ChangeEvent<HTMLSelectElement>) =>
    set(key, e.target.value || null);
  return (
    <>
      <h2 className="settings-section">Reading layout</h2>
      <p className="settings-intro">
        A font and layout to set on books, so they open the way you like without
        adjusting each one on your Kobo. Anything left as the Kobo sets it is
        unchanged. Select books in On Kobo and choose Set layout, or have it set
        for you below.
      </p>
      <Row label="Font" labelFor={font}>
        <select
          id={font}
          value={values.kobo_layout_font ?? ""}
          onChange={pick("kobo_layout_font")}
        >
          <option value="">As the Kobo sets it</option>
          {fonts.map((name) => (
            <option key={name} value={name}>
              {name}
            </option>
          ))}
        </select>
      </Row>
      <Row
        label="Font size"
        labelFor={size}
        hint="The number your Kobo shows for font size, from 10 to 60."
      >
        <NumberSetting
          {...props}
          name="kobo_layout_size"
          id={size}
          min={10}
          max={60}
          placeholder="As the Kobo sets it"
        />
      </Row>
      <Row label="Line spacing" labelFor={spacing}>
        <select
          id={spacing}
          value={values.kobo_layout_line_height ?? ""}
          onChange={pick("kobo_layout_line_height")}
        >
          <option value="">As the Kobo sets it</option>
          {LINE_HEIGHTS.map((h) => (
            <option key={h} value={h}>
              {h}
            </option>
          ))}
        </select>
      </Row>
      <Row label="Margins" labelFor={margins} hint="From 0 to 16, both sides.">
        <NumberSetting
          {...props}
          name="kobo_layout_margins"
          id={margins}
          min={0}
          max={16}
          placeholder="As the Kobo sets it"
        />
      </Row>
      <Row label="Alignment" labelFor={alignment}>
        <select
          id={alignment}
          value={values.kobo_layout_alignment ?? ""}
          onChange={pick("kobo_layout_alignment")}
        >
          <option value="">As the book sets it</option>
          <option value="left">Left</option>
          <option value="justify">Justified</option>
        </select>
      </Row>
      <Row label="Automatically">
        <Check
          {...props}
          name="kobo_layout"
          defaultOn={false}
          hint="When a Kobo connects, books from your library that have no font settings of their own get this layout. Books you've adjusted on the Kobo keep theirs."
        >
          Set this layout on new books
        </Check>
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
        <Check
          {...props}
          name="kobo_positions"
          hint="When you send a book again, or to another Kobo, it opens where you last were instead of at the start. Your place is saved each time a Kobo connects."
        >
          Put back your place in books sent again
        </Check>
        <Check
          {...props}
          name="kobo_firmware_check"
          defaultOn={false}
          hint="Asks Kobo once each time you connect. Kobo is told your model and software version, not your serial number."
        >
          Check for new Kobo software
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
      <SendingSection {...props} />
      <LayoutSection {...props} />
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
