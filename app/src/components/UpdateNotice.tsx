import { Download, X } from "lucide-react";
import { openHelp, type UpdateInfo } from "../api";
import type { UpdateStage } from "../updates";

interface Props {
  update: UpdateInfo;
  stage: UpdateStage;
  /** Something is writing to the Kobo or library, so restarting must wait. */
  busy: boolean;
  onInstall: () => void;
  onSkip: () => void;
  onClose: () => void;
}

function detail(update: UpdateInfo, stage: UpdateStage, busy: boolean) {
  switch (stage.stage) {
    case "installing":
      return stage.share === null
        ? "Downloading the update…"
        : `Downloading the update… ${Math.round(stage.share * 100)}%`;
    case "failed":
      return `The update couldn't be installed: ${stage.error}`;
    case "offered":
      return busy
        ? `You have ${update.current}. You can install it once the current task finishes.`
        : `You have ${update.current}. Installing restarts Parallax; your library and open books are kept.`;
  }
}

/** A banner under the toolbar offering a newer version of Parallax. */
export function UpdateNotice({
  update,
  stage,
  busy,
  onInstall,
  onSkip,
  onClose,
}: Props) {
  const installing = stage.stage === "installing";
  return (
    <section
      className={`kobo-notice ${stage.stage === "failed" ? "warn" : ""}`}
      aria-label="Update available"
    >
      <Download className="kobo-notice-icon" aria-hidden="true" />
      <div className="kobo-notice-text" aria-live="polite">
        <p className="kobo-notice-title">
          Parallax {update.version} is available
        </p>
        <p className="muted">{detail(update, stage, busy)}</p>
      </div>
      <div className="kobo-notice-actions">
        <button
          className="secondary"
          onClick={() => void openHelp("releases").catch(console.error)}
        >
          What's New
        </button>
        <button className="secondary" disabled={installing} onClick={onSkip}>
          Skip This Version
        </button>
        <button
          className="primary"
          disabled={busy || installing}
          onClick={onInstall}
        >
          {stage.stage === "failed" ? "Try Again" : "Install and Restart"}
        </button>
      </div>
      <button
        className="icon kobo-notice-close"
        aria-label="Close"
        title="Remind me later"
        disabled={installing}
        onClick={onClose}
      >
        <X />
      </button>
    </section>
  );
}
