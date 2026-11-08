import { useEffect, useId, useState } from "react";
import type { ReaderSettings } from "../api";
import { FONT_SIZES, resized } from "../reader/appearance";
import { RATES } from "../reader/speech";
import { Row } from "./panes";

type Key = "theme" | "flow" | "spread" | "width" | "margins" | "spacing";

const CHOICES: {
  key: Key;
  label: string;
  options: [string, string][];
}[] = [
  {
    key: "theme",
    label: "Theme",
    options: [
      ["auto", "Auto"],
      ["light", "Light"],
      ["sepia", "Sepia"],
      ["dark", "Dark"],
      ["contrast", "Contrast"],
    ],
  },
  {
    key: "flow",
    label: "Layout",
    options: [
      ["paginated", "Pages"],
      ["scrolled", "Scroll"],
    ],
  },
  {
    key: "spread",
    label: "Pages side by side",
    options: [
      ["auto", "When wide"],
      ["single", "Never"],
    ],
  },
  {
    key: "width",
    label: "Line length",
    options: [
      ["narrow", "Short"],
      ["medium", "Medium"],
      ["wide", "Long"],
    ],
  },
  {
    key: "margins",
    label: "Margins",
    options: [
      ["narrow", "Narrow"],
      ["medium", "Medium"],
      ["wide", "Wide"],
    ],
  },
  {
    key: "spacing",
    label: "Line spacing",
    options: [
      ["book", "Book"],
      ["relaxed", "Relaxed"],
      ["loose", "Loose"],
    ],
  },
];

/** The system's voices, which can arrive after the page loads. */
function useVoices(): SpeechSynthesisVoice[] {
  const [voices, setVoices] = useState<SpeechSynthesisVoice[]>([]);
  useEffect(() => {
    const speech = "speechSynthesis" in window ? window.speechSynthesis : null;
    if (!speech) return;
    const load = () =>
      setVoices(
        [...speech.getVoices()].sort(
          (a, b) =>
            a.lang.localeCompare(b.lang) || a.name.localeCompare(b.name),
        ),
      );
    load();
    speech.addEventListener?.("voiceschanged", load);
    return () => speech.removeEventListener?.("voiceschanged", load);
  }, []);
  return voices;
}

const languageName = (lang: string) => {
  try {
    return (
      new Intl.DisplayNames(undefined, { type: "language" }).of(lang) ?? lang
    );
  } catch {
    return lang;
  }
};

export function ReaderPane({
  settings,
  onChange,
}: {
  settings: ReaderSettings;
  onChange: (settings: ReaderSettings) => void;
}) {
  const voices = useVoices();
  const voiceId = useId();
  const rateId = useId();
  const byLanguage = new Map<string, SpeechSynthesisVoice[]>();
  for (const voice of voices)
    byLanguage.set(languageName(voice.lang), [
      ...(byLanguage.get(languageName(voice.lang)) ?? []),
      voice,
    ]);
  const missingVoice =
    settings.voice !== null &&
    voices.length > 0 &&
    !voices.some((v) => v.voiceURI === settings.voice);

  return (
    <>
      <p className="settings-intro">
        How books look in the reader. Changes show in open books too.
      </p>
      <Row label="Text size">
        <div className="stepper">
          <button
            aria-label="Smaller text"
            disabled={settings.fontSize <= FONT_SIZES[0]}
            onClick={() => onChange(resized(settings, -1))}
          >
            A−
          </button>
          <output aria-live="polite">{settings.fontSize}%</output>
          <button
            aria-label="Larger text"
            disabled={settings.fontSize >= FONT_SIZES[FONT_SIZES.length - 1]}
            onClick={() => onChange(resized(settings, 1))}
          >
            A+
          </button>
        </div>
      </Row>
      {CHOICES.map(({ key, label, options }) => (
        <Row
          key={key}
          label={label}
          hint={
            key === "theme"
              ? "Auto matches the app's appearance, set in General."
              : undefined
          }
        >
          <div className="segmented" role="radiogroup" aria-label={label}>
            {options.map(([value, text]) => (
              <button
                key={value}
                role="radio"
                aria-checked={settings[key] === value}
                className={settings[key] === value ? "active" : undefined}
                onClick={() => onChange({ ...settings, [key]: value })}
              >
                {text}
              </button>
            ))}
          </div>
        </Row>
      ))}
      <h2 className="settings-section">Read aloud</h2>
      <Row
        label="Voice"
        labelFor={voiceId}
        hint="A voice for another language than the book's is skipped, and the system's voice for the book reads instead."
      >
        <select
          id={voiceId}
          value={missingVoice ? "" : (settings.voice ?? "")}
          onChange={(e) =>
            onChange({ ...settings, voice: e.target.value || null })
          }
        >
          <option value="">System voice</option>
          {[...byLanguage].map(([language, list]) => (
            <optgroup key={language} label={language}>
              {list.map((voice) => (
                <option key={voice.voiceURI} value={voice.voiceURI}>
                  {voice.name}
                </option>
              ))}
            </optgroup>
          ))}
        </select>
      </Row>
      <Row label="Speed" labelFor={rateId}>
        <select
          id={rateId}
          value={settings.speechRate}
          onChange={(e) =>
            onChange({ ...settings, speechRate: Number(e.target.value) })
          }
        >
          {RATES.map((rate) => (
            <option key={rate} value={Math.round(rate * 100)}>
              {rate === 1 ? "Normal" : `${rate}×`}
            </option>
          ))}
        </select>
      </Row>
    </>
  );
}
