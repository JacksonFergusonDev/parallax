import { useEffect, useId, useState } from "react";
import {
  kindleEmailSettings,
  setKindleEmailSettings,
  type KindleEmailSettings,
} from "../api";
import { Check, Row, type PaneProps } from "./panes";

const EMPTY: KindleEmailSettings = {
  to: "",
  host: "",
  port: null,
  user: "",
  from: "",
  tls: false,
};

/** Send to Kindle by email: where books go and the account that sends them. */
function EmailSection() {
  const [saved, setSaved] = useState<KindleEmailSettings | null>(null);
  const [draft, setDraft] = useState<KindleEmailSettings>(EMPTY);
  const [passwordSaved, setPasswordSaved] = useState(false);
  const [password, setPassword] = useState("");
  const [status, setStatus] = useState<{ error: boolean; text: string } | null>(
    null,
  );
  const ids = {
    to: useId(),
    host: useId(),
    port: useId(),
    user: useId(),
    from: useId(),
    password: useId(),
  };

  useEffect(() => {
    let alive = true;
    kindleEmailSettings()
      .then(({ password_saved, ...settings }) => {
        if (!alive) return;
        setSaved(settings);
        setDraft(settings);
        setPasswordSaved(password_saved);
      })
      .catch((e) => alive && setStatus({ error: true, text: String(e) }));
    return () => {
      alive = false;
    };
  }, []);

  const field = (key: "to" | "host" | "user" | "from") => ({
    value: draft[key],
    onChange: (e: React.ChangeEvent<HTMLInputElement>) =>
      setDraft({ ...draft, [key]: e.target.value }),
    autoComplete: "off",
    autoCapitalize: "off",
    autoCorrect: "off",
    spellCheck: false,
  });

  const changed =
    saved !== null &&
    (JSON.stringify(saved) !== JSON.stringify(draft) || password !== "");

  const save = async (forgetPassword = false) => {
    setStatus(null);
    try {
      await setKindleEmailSettings(
        draft,
        forgetPassword ? "" : password || undefined,
      );
      setSaved(draft);
      setPassword("");
      setPasswordSaved(forgetPassword ? false : passwordSaved || !!password);
      setStatus({ error: false, text: "Saved." });
    } catch (e) {
      setStatus({ error: true, text: String(e) });
    }
  };

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <Row
        label="Your Kindle's email"
        labelFor={ids.to}
        hint="Find it on Amazon under Manage Your Content and Devices › Preferences › Personal Document Settings. Add the address you send from to the Approved Personal Document E-mail List there, or Amazon drops the books."
      >
        <div className="settings-inline">
          <input
            id={ids.to}
            type="email"
            placeholder="name_abc@kindle.com"
            {...field("to")}
          />
        </div>
      </Row>
      <Row
        label="Outgoing mail server"
        labelFor={ids.host}
        hint="Your email provider's SMTP server, such as smtp.gmail.com or smtp.mail.me.com."
      >
        <div className="settings-inline">
          <input
            id={ids.host}
            placeholder="smtp.example.com"
            {...field("host")}
          />
          <input
            id={ids.port}
            aria-label="Port"
            type="number"
            inputMode="numeric"
            className="settings-number"
            min={1}
            max={65535}
            placeholder={draft.tls ? "465" : "587"}
            value={draft.port ?? ""}
            onChange={(e) =>
              setDraft({
                ...draft,
                port: e.target.value ? Number(e.target.value) : null,
              })
            }
          />
        </div>
        <div className="settings-check">
          <label>
            <input
              type="checkbox"
              checked={draft.tls}
              onChange={(e) => setDraft({ ...draft, tls: e.target.checked })}
            />
            Use TLS from the start (port 465)
          </label>
          <p className="hint">
            Off uses STARTTLS on port 587, which most providers prefer.
          </p>
        </div>
      </Row>
      <Row label="User name" labelFor={ids.user}>
        <div className="settings-inline">
          <input
            id={ids.user}
            placeholder="you@example.com"
            {...field("user")}
          />
        </div>
      </Row>
      <Row
        label="Password"
        labelFor={ids.password}
        hint="Many providers need an app password made for Parallax, not your usual one. It's kept in your Mac's keychain."
      >
        <div className="settings-inline">
          <input
            id={ids.password}
            type="password"
            autoComplete="off"
            placeholder={passwordSaved ? "Saved" : "App password"}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
          {passwordSaved && (
            <button
              type="button"
              className="danger"
              onClick={() => void save(true)}
            >
              Remove
            </button>
          )}
        </div>
      </Row>
      <Row
        label="Send from"
        labelFor={ids.from}
        hint="Only if it isn't your user name. It must be on Amazon's approved list."
      >
        <div className="settings-inline">
          <input
            id={ids.from}
            type="email"
            placeholder={draft.user || "you@example.com"}
            {...field("from")}
          />
        </div>
      </Row>
      <Row label="">
        <div className="settings-inline">
          <button type="submit" disabled={!changed}>
            Save
          </button>
          {status && (
            <span
              className={`notice notice-${status.error ? "error" : "info"}`}
            >
              {status.text}
            </span>
          )}
        </div>
      </Row>
    </form>
  );
}

export function KindlePane(props: PaneProps) {
  return (
    <>
      <p className="settings-intro muted">
        Kindle support is new and hasn't been tested on a Kindle yet. You can
        send books by cable, or by email to any Kindle.
      </p>
      <Row label="Sending by cable">
        <Check
          {...props}
          name="kindle_page_numbers"
          hint="Estimated page numbers, one page per 2,300 characters, as calibre makes them. They won't match a printed edition."
        >
          Add page numbers
        </Check>
      </Row>
      <Row label="Connecting">
        <Check
          {...props}
          name="kindle_free_from_photos"
          hint="Kindles from 2024 on, and the Scribe, connect without appearing as a drive. macOS's photo import service grabs them first, so Parallax stops it while it opens your Kindle. macOS starts it again when it's needed."
        >
          Free your Kindle from photo import
        </Check>
      </Row>
      <h2 className="settings-section">Send to Kindle by email</h2>
      <EmailSection />
    </>
  );
}
