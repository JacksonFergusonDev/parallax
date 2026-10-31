import { memo } from "react";
import { coverSrc, type Book } from "../api";
import type { SendStatus } from "../library";

const STATUS_LABEL: Record<SendStatus["stage"], string> = {
  queued: "Waiting…",
  preparing: "Preparing…",
  converting: "Converting…",
  copying: "Copying…",
  sent: "Sent",
  failed: "Failed",
};

interface Props {
  book: Book;
  selected: boolean;
  /** On the connected Kobo; "stale" if edited since it was sent. */
  deviceState: "on" | "stale" | undefined;
  status: SendStatus | undefined;
  onSelect: (id: number, event: React.MouseEvent) => void;
}

export const BookCard = memo(function BookCard({
  book,
  selected,
  deviceState,
  status,
  onSelect,
}: Props) {
  const src = coverSrc(book);
  return (
    <button
      className={`card ${selected ? "selected" : ""}`}
      onClick={(e) => onSelect(book.id, e)}
      title={status?.stage === "failed" ? status.error : undefined}
    >
      <div className="cover">
        {src ? (
          <img src={src} alt="" loading="lazy" draggable={false} />
        ) : (
          <div className="cover-placeholder">
            <span>{book.title}</span>
          </div>
        )}
        {status && (
          <span className={`overlay overlay-${status.stage}`}>
            {STATUS_LABEL[status.stage]}
          </span>
        )}
        {!status && deviceState && (
          <span className={`on-device ${deviceState}`}>
            {deviceState === "stale" ? "Update available" : "On Kobo"}
          </span>
        )}
      </div>
      <div className="card-title">{book.title}</div>
      <div className="card-author">
        {book.authors.join(", ") || "Unknown author"}
      </div>
    </button>
  );
});
