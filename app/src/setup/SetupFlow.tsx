// First-run setup: connect a Kobo, bring its books in, choose how Parallax
// looks after it, then a short tour. It takes over the main window until
// it's finished or skipped; see setup.ts for when it shows.
import { useCallback, useEffect, useId, useRef, useState } from "react";
import {
  BookCheck,
  BookOpen,
  Check as CheckIcon,
  FileUp,
  FolderOpen,
  Info,
  LibraryBig,
  Send,
  ShieldCheck,
  Tablet,
  TriangleAlert,
  Wrench,
} from "lucide-react";
import {
  deviceBooks,
  importFromDevice,
  koboHealth,
  type DeviceInfo,
} from "../api";
import { appTheme } from "../design/theme";
import { koboNotice, rememberKoboNotice } from "../koboNotice";
import { summarizeDeviceImport } from "../device";
import { useBookProgress } from "../bookProgress";
import { BookProgressBar } from "../components/BookProgressBar";
import { ThemePicker } from "../components/ThemePicker";
import { Check, Choices } from "../settings/panes";
import { useSettings } from "../settings/useSettings";
import {
  COUNTED,
  customised,
  healthIssues,
  nextStep,
  previousStep,
  type SetupOutcome,
  type Step,
} from "./setup";
import "./setup.css";

interface Props {
  device: DeviceInfo | null;
  /** Books in the library now. */
  libraryBooks: number;
  /** Library books already on the connected Kobo. */
  onKobo: number;
  /** The main window is adding books the user chose. */
  adding: boolean;
  onLibraryChanged: () => Promise<void>;
  onChooseFiles: () => void;
  onChooseComicFolder: () => void;
  onCalibre: () => void;
  onGoodreads: () => void;
  onFinish: (outcome: SetupOutcome) => void;
}

const plural = (n: number, word: string) =>
  `${n.toLocaleString()} ${n === 1 ? word : `${word}s`}`;

export function SetupFlow(props: Props) {
  const { device, onFinish } = props;
  const [step, setStep] = useState<Step>("welcome");
  /** The Kobo seen during setup, for the summary once it's unplugged. */
  const [kobo, setKobo] = useState<string | null>(null);
  /** What adding the Kobo's books did, kept when moving between steps. */
  const [imported, setImported] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** Customising rather than taking the recommended settings; until the
   * user picks, it follows whether any setting is already changed. */
  const [custom, setCustom] = useState<boolean | null>(null);
  const settings = useSettings(setError);
  const heading = useRef<HTMLHeadingElement>(null);
  const titleId = useId();

  // Remembered while rendering, so the summary names it after unplugging.
  if (device && device.model !== kobo) setKobo(device.model);

  // Each step announces itself and puts the keyboard at its start.
  const first = useRef(true);
  useEffect(() => {
    if (first.current) {
      first.current = false;
      return;
    }
    heading.current?.focus();
  }, [step]);

  const waiting = step === "connect" && !device;
  const forward = useCallback(() => {
    if (step === "done") onFinish("done");
    else setStep(nextStep(step));
  }, [step, onFinish]);

  const primary =
    step === "welcome"
      ? "Get Started"
      : step === "done"
        ? "Open Library"
        : "Continue";

  const onKeyDown = (e: React.KeyboardEvent) => {
    // Return continues unless a control has it; Escape never skips setup.
    if (e.key !== "Enter" || e.metaKey || e.ctrlKey || e.altKey) return;
    if ((e.target as HTMLElement).closest("button, input, select, a")) return;
    if (waiting) return;
    e.preventDefault();
    forward();
  };

  const counted = COUNTED.indexOf(step);

  return (
    <div className="setup" onKeyDown={onKeyDown}>
      <section
        className="setup-card"
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <div className="setup-body" key={step}>
          {step === "welcome" && (
            <Welcome titleId={titleId} heading={heading} />
          )}
          {step === "connect" && (
            <Connect titleId={titleId} heading={heading} device={device} />
          )}
          {step === "books" && (
            <Books
              {...props}
              titleId={titleId}
              heading={heading}
              imported={imported}
              onImported={setImported}
            />
          )}
          {step === "settings" && settings.values && (
            <SettingsStep
              titleId={titleId}
              heading={heading}
              values={settings.values}
              set={settings.set}
              custom={custom ?? customised(settings.values).length > 0}
              onCustom={setCustom}
            />
          )}
          {step === "appearance" && (
            <Appearance
              titleId={titleId}
              heading={heading}
              theme={appTheme(settings.values?.theme)}
              onTheme={(next) =>
                settings.set("theme", next === "system" ? null : next)
              }
            />
          )}
          {step === "tour" && <Tour titleId={titleId} heading={heading} />}
          {step === "done" && (
            <Done
              titleId={titleId}
              heading={heading}
              kobo={kobo}
              imported={imported}
              recommended={
                !!settings.values && customised(settings.values).length === 0
              }
              libraryBooks={props.libraryBooks}
            />
          )}
          {error && (
            <p className="notice notice-error setup-error" role="alert">
              {error}
            </p>
          )}
        </div>
        <footer className="setup-footer">
          {step !== "done" ? (
            <button
              className="setup-skip"
              onClick={() => onFinish("skipped")}
              title="Setup won't show again. Run it any time from Help › Welcome to Parallax."
            >
              Skip Setup
            </button>
          ) : (
            <span />
          )}
          {counted >= 0 && (
            <ol className="setup-dots" aria-label="Setup progress">
              {COUNTED.map((s, i) => (
                <li
                  key={s}
                  className={i < counted ? "past" : i === counted ? "now" : ""}
                  aria-current={i === counted ? "step" : undefined}
                >
                  <span className="visually-hidden">
                    Step {i + 1} of {COUNTED.length}
                  </span>
                </li>
              ))}
            </ol>
          )}
          <div className="setup-nav">
            {step !== "welcome" && step !== "done" && (
              <button onClick={() => setStep(previousStep(step))}>Back</button>
            )}
            {waiting ? (
              <button key="without" onClick={() => setStep("books")}>
                Continue Without a Kobo
              </button>
            ) : (
              <button key="primary" className="primary" onClick={forward}>
                {primary}
              </button>
            )}
          </div>
        </footer>
      </section>
    </div>
  );
}

const bookCount = (mount: string) =>
  deviceBooks(mount).then((books) => books.length);
const issueCount = (mount: string) => koboHealth(mount).then(healthIssues);

/**
 * Reads something from `device`, kept with the Kobo it came from so another
 * Kobo, or none, never shows it. Undefined until it's read.
 */
function useKoboRead<T>(
  device: DeviceInfo | null,
  read: (mount: string) => Promise<T>,
) {
  const [result, setResult] = useState<{
    mount: string;
    value?: T;
    error?: string;
  } | null>(null);
  const mount = device?.mount ?? null;
  useEffect(() => {
    if (!mount) return;
    let alive = true;
    read(mount).then(
      (value) => alive && setResult({ mount, value }),
      (e) => alive && setResult({ mount, error: String(e) }),
    );
    return () => {
      alive = false;
    };
  }, [mount, read]);
  return result && result.mount === mount ? result : undefined;
}

interface StepProps {
  titleId: string;
  heading: React.RefObject<HTMLHeadingElement | null>;
}

function Title({
  titleId,
  heading,
  children,
}: StepProps & { children: React.ReactNode }) {
  return (
    <h1 id={titleId} ref={heading} tabIndex={-1} className="setup-title">
      {children}
    </h1>
  );
}

function Welcome(props: StepProps) {
  return (
    <div className="setup-welcome">
      <img className="setup-mark" src="/parallax.svg" alt="" />
      <Title {...props}>Welcome to Parallax</Title>
      <p className="setup-lead">
        Your books, your Kobo and your reading, kept in step. A few quick steps
        will connect your Kobo, bring its books into your library and set things
        up the way you like.
      </p>
    </div>
  );
}

/** A computer and a Kobo, joined once the Kobo is found. */
function ConnectArt({ connected }: { connected: boolean }) {
  return (
    <svg
      className={`setup-art ${connected ? "connected" : ""}`}
      viewBox="0 0 280 120"
      aria-hidden="true"
    >
      <rect className="art-line" x="8" y="18" width="128" height="80" rx="6" />
      <rect className="art-fill" x="16" y="26" width="112" height="64" rx="2" />
      <path className="art-line" d="M0 104h144" />
      <rect className="art-line" x="204" y="12" width="68" height="96" rx="8" />
      <rect className="art-fill" x="212" y="22" width="52" height="70" rx="2" />
      <path className="art-text" d="M220 34h36M220 42h36M220 50h28M220 58h32" />
      <path className="art-cable" d="M144 92c24 0 20 20 46 20h48v-4" />
      <circle className="art-pulse" cx="238" cy="100" r="3" />
    </svg>
  );
}

function Connect({
  device,
  ...props
}: StepProps & { device: DeviceInfo | null }) {
  const count = useKoboRead(device, bookCount)?.value ?? null;
  const writable = device && device.read_only === null ? device : null;
  const issues = useKoboRead(writable, issueCount)?.value ?? null;
  const notice = device && koboNotice(device);
  const readOnly = !!device?.read_only;
  // Said here, so the main window needn't say it again after setup.
  const noticeFor = notice && device ? device.model : null;
  useEffect(() => {
    if (noticeFor) void rememberKoboNotice(noticeFor).catch(console.error);
  }, [noticeFor]);

  return (
    <>
      <Title {...props}>
        {device ? "Your Kobo is connected" : "Connect your Kobo"}
      </Title>
      <ConnectArt connected={!!device} />
      {device ? (
        <div className="setup-device">
          <Tablet aria-hidden="true" />
          <div>
            <p className="setup-device-name">{device.model}</p>
            <p className="muted">
              Software {device.firmware}
              {count !== null && ` · ${plural(count, "book")}`}
            </p>
          </div>
        </div>
      ) : (
        <ol className="setup-steps">
          <li>Plug your Kobo into this Mac with a USB cable.</li>
          <li>
            On the Kobo, tap <strong>Connect</strong> when it asks to connect to
            your computer.
          </li>
          <li>Parallax finds it on its own, usually within a few seconds.</li>
        </ol>
      )}
      {notice && (
        <p className={`setup-note ${readOnly ? "warn" : "info"}`}>
          {readOnly ? (
            <TriangleAlert aria-hidden="true" />
          ) : (
            <Info aria-hidden="true" />
          )}
          <span>
            <strong>{notice.title}.</strong> {notice.body}
          </span>
        </p>
      )}
      {issues !== null && (
        <p className={`setup-note ${issues === 0 ? "" : "info"}`}>
          {issues === 0 ? (
            <CheckIcon aria-hidden="true" />
          ) : (
            <Wrench aria-hidden="true" />
          )}
          <span>
            {issues === 0
              ? "Parallax checked your Kobo and everything looks good."
              : `Parallax found ${plural(issues, "small thing")} worth tidying on your Kobo. Check This Kobo… in the Kobo menu fixes them.`}
          </span>
        </p>
      )}
      <p className="setup-status" role="status">
        {device ? (
          ""
        ) : (
          <>
            <span className="setup-spinner" aria-hidden="true" />
            Waiting for your Kobo…
          </>
        )}
      </p>
    </>
  );
}

function Books({
  device,
  libraryBooks,
  onKobo,
  adding,
  onLibraryChanged,
  onChooseFiles,
  onChooseComicFolder,
  onCalibre,
  onGoodreads,
  imported,
  onImported,
  ...props
}: Props &
  StepProps & {
    imported: string | null;
    onImported: (summary: string) => void;
  }) {
  const books = useKoboRead(device, bookCount);
  const total = books?.value ?? null;
  const [failed, setFailed] = useState<string | null>(null);
  const [operation, run] = useBookProgress();

  const addAll = async () => {
    if (!device) return;
    setFailed(null);
    try {
      const outcomes = await run("Adding", (report) =>
        importFromDevice(device.mount, null, report),
      );
      await onLibraryChanged();
      onImported(summarizeDeviceImport(outcomes));
    } catch (e) {
      setFailed(String(e));
    }
  };

  const others = (
    <>
      <div className="setup-sources">
        <button onClick={onChooseFiles} disabled={adding}>
          <FileUp aria-hidden="true" />
          Choose Files…
        </button>
        <button onClick={onChooseComicFolder} disabled={adding}>
          <FolderOpen aria-hidden="true" />
          Add Comic Folder…
        </button>
        <button onClick={onCalibre} disabled={adding}>
          <LibraryBig aria-hidden="true" />
          Import from calibre…
        </button>
        <button onClick={onGoodreads} disabled={adding}>
          <BookCheck aria-hidden="true" />
          Import from Goodreads…
        </button>
      </div>
      <p className="setup-hint muted">
        Optional: import your Goodreads export to see which of your books you've
        read. You can do this later from the Library tools menu.
      </p>
    </>
  );

  const newOnKobo = total === null ? null : Math.max(0, total - onKobo);

  return (
    <>
      <Title {...props}>
        {device ? "Bring your Kobo's books home" : "Add your books"}
      </Title>
      {device ? (
        <>
          <p className="setup-lead">
            Parallax keeps a copy of each book in your library on this Mac, so
            you can read, organise and send them back whenever you like. Nothing
            on your Kobo changes.
          </p>
          <div className="setup-panel" aria-live="polite">
            {operation ? (
              <BookProgressBar operation={operation} />
            ) : imported ? (
              <p className="setup-done-line">
                <CheckIcon aria-hidden="true" />
                {imported}
              </p>
            ) : total === null ? (
              <p className="muted">Looking at your Kobo…</p>
            ) : total === 0 ? (
              <p>Your Kobo doesn't have any books yet.</p>
            ) : newOnKobo === 0 ? (
              <p className="setup-done-line">
                <CheckIcon aria-hidden="true" />
                Every book on your Kobo is already in your library.
              </p>
            ) : (
              <div className="setup-offer">
                <p>
                  Your Kobo has {plural(total, "book")}
                  {onKobo > 0 &&
                    `, ${onKobo.toLocaleString()} already in your library`}
                  .
                </p>
                <button onClick={() => void addAll()}>
                  Add {plural(newOnKobo ?? 0, "Book")} to Library
                </button>
              </div>
            )}
          </div>
          <p className="setup-hint muted">
            Books bought from the Kobo Store are usually copy-protected, so they
            stay on the Kobo. You can also add books from this Mac:
          </p>
          {others}
        </>
      ) : (
        <>
          <p className="setup-lead">
            Add EPUBs, DRM-free Kindle books, comics and archives of books. You
            can also drop files anywhere on this window.
          </p>
          {others}
          <p className="setup-status" role="status">
            {adding
              ? "Adding books…"
              : libraryBooks > 0
                ? `${plural(libraryBooks, "book")} in your library.`
                : ""}
          </p>
        </>
      )}
      {(failed ?? books?.error) && (
        <p className="notice notice-error" role="alert">
          {failed ?? books?.error}
        </p>
      )}
    </>
  );
}

type SettingsProps = ReturnType<typeof useSettings>;

function SettingsStep({
  values,
  set,
  custom,
  onCustom,
  ...props
}: StepProps & {
  values: NonNullable<SettingsProps["values"]>;
  set: SettingsProps["set"];
  custom: boolean;
  onCustom: (custom: boolean) => void;
}) {
  const openId = useId();
  const comicsId = useId();
  const recommend = () => {
    for (const key of customised(values)) set(key, null);
    onCustom(false);
  };
  const choice = (
    on: boolean,
    pick: () => void,
    name: string,
    hint: string,
  ) => (
    <label className={`setup-choice ${on ? "on" : ""}`}>
      <input type="radio" name="setup-settings" checked={on} onChange={pick} />
      <span>
        <span className="setup-choice-name">{name}</span>
        <span className="setup-choice-hint">{hint}</span>
      </span>
    </label>
  );

  return (
    <>
      <Title {...props}>Choose your settings</Title>
      <div className="setup-choices" role="radiogroup" aria-label="Settings">
        {choice(
          !custom,
          recommend,
          "Recommended settings",
          "What suits most people. You can change anything later in Settings.",
        )}
        {choice(
          custom,
          () => onCustom(true),
          "Customise",
          "Choose how Parallax looks after your Kobo, new books and reading.",
        )}
      </div>
      {custom && (
        <div className="setup-custom">
          <fieldset>
            <legend>Your Kobo</legend>
            <Check
              values={values}
              set={set}
              name="kobo_series"
              hint="The Kobo ignores series in books copied to it, so Parallax adds them once the Kobo has imported them."
            >
              Add series to books when a Kobo connects
            </Check>
            <Check
              values={values}
              set={set}
              name="kobo_rules"
              hint="Your grouping rules file books into the Kobo's existing collections after sending and when it connects."
            >
              Apply grouping rules automatically
            </Check>
            <Check
              values={values}
              set={set}
              name="eject_after_send"
              defaultOn={false}
              hint="So the Kobo is ready to unplug once every book has been sent."
            >
              Eject the Kobo after sending books
            </Check>
          </fieldset>
          <fieldset>
            <legend>New books</legend>
            <Check
              values={values}
              set={set}
              name="review_on_add"
              hint="Points out missing covers, series and duplicates after books are added."
            >
              Review books when they're added
            </Check>
            <div className="setup-field">
              <label htmlFor={comicsId}>Import comics as</label>
              <select
                id={comicsId}
                value={values.comics_kind ?? "manga"}
                onChange={(e) =>
                  set(
                    "comics_kind",
                    e.target.value === "comic" ? "comic" : null,
                  )
                }
              >
                <option value="manga">Manga (right to left)</option>
                <option value="comic">Comics (left to right)</option>
              </select>
              <p className="hint">Used when a comic doesn't say which it is.</p>
            </div>
          </fieldset>
          <fieldset>
            <legend>Reading</legend>
            <div className="setup-field">
              <label htmlFor={openId}>Open Parallax to</label>
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
            </div>
            <Choices
              name="setup-reader-windows"
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
          </fieldset>
        </div>
      )}
      <p className="setup-note">
        <ShieldCheck aria-hidden="true" />
        <span>
          Before Parallax changes anything on a Kobo, it saves a backup of the
          Kobo's database. Backups are listed in Settings › Backups.
        </span>
      </p>
    </>
  );
}

function Appearance({
  theme,
  onTheme,
  ...props
}: StepProps & {
  theme: ReturnType<typeof appTheme>;
  onTheme: (theme: ReturnType<typeof appTheme>) => void;
}) {
  return (
    <>
      <Title {...props}>Choose a look</Title>
      <p className="setup-lead">
        Every window follows it, and books open in matching colours. System and
        High contrast switch between light and dark with macOS.
      </p>
      <div className="setup-themes">
        <ThemePicker value={theme} onChange={onTheme} />
      </div>
    </>
  );
}

const TOUR = [
  {
    icon: LibraryBig,
    name: "Library",
    keys: "⌘1",
    text: "Every book you own. Select one to read it, edit its details or find a better cover.",
  },
  {
    icon: Tablet,
    name: "On Kobo",
    keys: "⌘2",
    text: "Appears while your Kobo is connected: what's on it, its collections and its health.",
  },
  {
    icon: BookOpen,
    name: "Reading",
    keys: "⌘3",
    text: "What you're reading, how far you've got and how much time you've spent.",
  },
  {
    icon: Send,
    name: "Send to Kobo",
    keys: "⌘E to eject",
    text: "Select books and send them. Eject before unplugging so the Kobo can add them.",
  },
];

function Tour(props: StepProps) {
  return (
    <>
      <Title {...props}>Finding your way around</Title>
      <ul className="setup-tour">
        {TOUR.map(({ icon: Icon, name, keys, text }) => (
          <li key={name}>
            <span className="setup-tour-icon">
              <Icon aria-hidden="true" />
            </span>
            <div>
              <p className="setup-tour-name">
                {name} <kbd>{keys}</kbd>
              </p>
              <p className="muted">{text}</p>
            </div>
          </li>
        ))}
      </ul>
      <p className="setup-hint muted">
        Add books with ⌘O or by dropping them on the window. Settings (⌘,) has
        everything else.
      </p>
    </>
  );
}

function Done({
  kobo,
  imported,
  recommended,
  libraryBooks,
  ...props
}: StepProps & {
  kobo: string | null;
  imported: string | null;
  recommended: boolean;
  libraryBooks: number;
}) {
  const lines = [
    kobo && `${kobo} set up`,
    imported,
    recommended ? "Recommended settings" : "Your own settings",
    libraryBooks > 0 && `${plural(libraryBooks, "book")} in your library`,
  ].filter((line): line is string => Boolean(line));
  return (
    <div className="setup-welcome">
      <span className="setup-done-mark">
        <CheckIcon aria-hidden="true" />
      </span>
      <Title {...props}>You're all set</Title>
      <ul className="setup-summary">
        {lines.map((line) => (
          <li key={line}>{line}</li>
        ))}
      </ul>
      <p className="setup-hint muted">
        You can run setup again from Help › Welcome to Parallax.
      </p>
    </div>
  );
}
