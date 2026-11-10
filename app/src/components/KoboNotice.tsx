import { Info, TriangleAlert, X } from "lucide-react";
import { openHelp } from "../api";
import type { KoboStanding } from "../koboNotice";

interface Props {
  notice: { title: string; body: string };
  standing: KoboStanding | null;
  onClose: () => void;
  onDismiss: () => void;
}

/** A banner under the toolbar about an untested or read-only Kobo. */
export function KoboNotice({ notice, standing, onClose, onDismiss }: Props) {
  const Icon = standing === "read_only" ? TriangleAlert : Info;
  return (
    <section
      className={`kobo-notice ${standing === "read_only" ? "warn" : ""}`}
      aria-label="About this Kobo"
    >
      <Icon className="kobo-notice-icon" aria-hidden="true" />
      <div className="kobo-notice-text">
        <p className="kobo-notice-title">{notice.title}</p>
        <p className="muted">{notice.body}</p>
      </div>
      <div className="kobo-notice-actions">
        <button
          className="secondary"
          onClick={() => void openHelp("devices").catch(console.error)}
        >
          Supported Models
        </button>
        <button className="secondary" onClick={onDismiss}>
          Don't Show Again
        </button>
      </div>
      <button
        className="icon kobo-notice-close"
        aria-label="Close"
        title="Close until this Kobo reconnects"
        onClick={onClose}
      >
        <X />
      </button>
    </section>
  );
}
