import { useEffect, useId, useState } from "react";
import { getSetting, setSetting, type ExportRequest } from "../api";
import {
  DEFAULT_CHOICES,
  ORGANIZE,
  describeChoices,
  parseChoices,
  templateFor,
  templateProblem,
  type ExportChoices,
  type OrganizeId,
} from "../exportOptions";

interface Props {
  /** How many books will be exported. */
  count: number;
  /** Whether those are the selected books rather than every book shown. */
  selection: boolean;
  onExport: (request: ExportRequest) => void;
  onClose: () => void;
}

/** Chooses how Export Library writes books, with defaults that need no
 * changes, then hands the choices on for a destination to be picked. */
export function ExportLibrary({ count, selection, onExport, onClose }: Props) {
  const [choices, setChoices] = useState<ExportChoices>(DEFAULT_CHOICES);
  const [settings, setSettings] = useState(false);
  const [customize, setCustomize] = useState(false);
  const organizeId = useId();
  const templateId = useId();
  const templateHint = useId();
  const formatHint = useId();
  const settingsHint = useId();

  useEffect(() => {
    let alive = true;
    getSetting("export_options")
      .then((saved) => alive && setChoices(parseChoices(saved)))
      .catch(() => undefined);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => {
      alive = false;
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  const change = (next: Partial<ExportChoices>) =>
    setChoices((prev) => ({ ...prev, ...next }));
  const template = templateFor(choices);
  const problem = templateProblem(template);
  const books = count === 1 ? "1 book" : `${count} books`;

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (problem) {
      setCustomize(true);
      return;
    }
    void setSetting("export_options", JSON.stringify(choices)).catch(
      () => undefined,
    );
    onExport({
      archive: choices.archive,
      template,
      format: choices.format,
      settings: choices.archive && settings,
    });
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <form
        className="modal export"
        onSubmit={submit}
        onClick={(e) => e.stopPropagation()}
        aria-labelledby={`${organizeId}-title`}
      >
        <h2 id={`${organizeId}-title`}>
          {selection ? `Export ${books}` : `Export your library (${books})`}
        </h2>
        <p className="muted">
          Your books with your edits and covers, ready to share or keep as a
          backup. Parallax can import the file again.
        </p>
        <p className="export-summary">{describeChoices(choices)}</p>
        <button
          type="button"
          className="link export-toggle"
          aria-expanded={customize}
          onClick={() => setCustomize((open) => !open)}
        >
          {customize ? "Hide options" : "Customise…"}
        </button>
        {customize && (
          <div className="export-options">
            <div className="export-field" role="group" aria-label="Save as">
              <span className="export-label">Save as</span>
              <div className="segmented">
                {[true, false].map((archive) => (
                  <button
                    key={String(archive)}
                    type="button"
                    className={choices.archive === archive ? "active" : ""}
                    aria-pressed={choices.archive === archive}
                    onClick={() => change({ archive })}
                  >
                    {archive ? "ZIP file" : "Folder"}
                  </button>
                ))}
              </div>
            </div>
            <div className="export-field">
              <label className="export-label" htmlFor={organizeId}>
                Organise by
              </label>
              <select
                id={organizeId}
                value={choices.organize}
                onChange={(e) =>
                  change({ organize: e.target.value as OrganizeId })
                }
              >
                {ORGANIZE.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.label}
                  </option>
                ))}
                <option value="custom">Custom</option>
              </select>
            </div>
            {choices.organize === "custom" && (
              <div className="export-field">
                <label className="export-label" htmlFor={templateId}>
                  Template
                </label>
                <input
                  id={templateId}
                  aria-describedby={templateHint}
                  aria-invalid={problem !== null}
                  spellCheck={false}
                  autoCapitalize="off"
                  autoCorrect="off"
                  value={choices.template}
                  onChange={(e) => change({ template: e.target.value })}
                />
                <p
                  className={problem ? "hint export-problem" : "hint"}
                  id={templateHint}
                >
                  {problem ??
                    "Use {title}, {author}, {authors}, {series}, {series_index}, {genre}, {publisher}, {year}, {language} and {id}. A / starts a folder. Text in [ ] is left out when a field in it is empty."}
                </p>
              </div>
            )}
            <div
              className="export-field"
              role="group"
              aria-label="Format"
              aria-describedby={formatHint}
            >
              <span className="export-label">Format</span>
              <div className="segmented">
                {(["epub", "kepub"] as const).map((format) => (
                  <button
                    key={format}
                    type="button"
                    className={choices.format === format ? "active" : ""}
                    aria-pressed={choices.format === format}
                    onClick={() => change({ format })}
                  >
                    {format === "epub" ? "EPUB" : "KEPUB"}
                  </button>
                ))}
              </div>
              <p className="hint" id={formatHint}>
                {choices.format === "epub"
                  ? "Opens in any reading app or e-reader."
                  : "Kobo's format, converted as books sent to a Kobo are. Best when the books are going onto a Kobo."}{" "}
                Comics stay CBZ.
              </p>
            </div>
            <div className="export-field">
              <label className="export-check">
                <input
                  type="checkbox"
                  checked={choices.archive && settings}
                  disabled={!choices.archive}
                  aria-describedby={settingsHint}
                  onChange={(e) => setSettings(e.target.checked)}
                />
                Include my Parallax settings
              </label>
              <p className="hint" id={settingsHint}>
                {choices.archive
                  ? "Importing the file into Parallax offers to use them. Reading history, passwords and keys are never included."
                  : "Only a ZIP file can include settings."}
              </p>
            </div>
          </div>
        )}
        <div className="form-actions">
          <button
            type="button"
            className="link export-reset"
            hidden={
              !customize ||
              JSON.stringify(choices) === JSON.stringify(DEFAULT_CHOICES)
            }
            onClick={() => {
              setChoices(DEFAULT_CHOICES);
              setSettings(false);
            }}
          >
            Use defaults
          </button>
          <button type="button" className="secondary" onClick={onClose}>
            Cancel
          </button>
          <button
            type="submit"
            className="primary"
            disabled={count === 0 || (customize && problem !== null)}
          >
            Export…
          </button>
        </div>
      </form>
    </div>
  );
}
