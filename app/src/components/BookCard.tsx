import { memo } from "react";
import { TransferProgress } from "./TransferProgress";
import { GeneratedCover } from "./GeneratedCover";
import { coverSrc, type Book } from "../api";
import {
  deviceStateLabels,
  progressLabel,
  readState,
  sendStatusLabel,
  type DeviceState,
  type SendStatus,
} from "../library";

interface Props {
  book: Book;
  selected: boolean;
  /** Presence and changes on the connected Kobo. */
  deviceState: DeviceState;
  /** "Kobo" or "Kindle", for the status label. */
  deviceName?: string;
  status: SendStatus | undefined;
  onSelect: (id: number, event: React.MouseEvent) => void;
}

export const BookCard = memo(function BookCard({
  book,
  selected,
  deviceState,
  deviceName,
  status,
  onSelect,
}: Props) {
  const src = coverSrc(book);
  return (
    <button
      className={`card ${selected ? "selected" : ""}`}
      aria-pressed={selected}
      aria-label={`${book.title}${book.authors[0] ? ` by ${book.authors[0]}` : ""}`}
      onClick={(e) => onSelect(book.id, e)}
      aria-description={
        status
          ? sendStatusLabel(status)
          : deviceState && deviceStateLabels(deviceName)[deviceState]
      }
      title={status?.stage === "failed" ? status.error : undefined}
    >
      <div className="cover">
        {src ? (
          <img
            src={src}
            alt=""
            loading="eager"
            decoding="sync"
            draggable={false}
          />
        ) : (
          <GeneratedCover title={book.title} author={book.authors[0]} />
        )}
        {status &&
          status.stage !== "queued" &&
          status.stage !== "failed" &&
          status.stage !== "sent" && (
            <span className="cover-sheen" aria-hidden="true" />
          )}
        {status?.stage === "copying" && <TransferProgress status={status} />}
        {status && (
          <span className={`overlay overlay-${status.stage}`}>
            {sendStatusLabel(status)}
          </span>
        )}
        {readState(book) !== "unread" && (
          <span className={`read-badge ${readState(book)}`}>
            {progressLabel(book)}
          </span>
        )}
        {!status && deviceState && (
          <span className={`on-device ${deviceState}`}>
            {deviceStateLabels(deviceName)[deviceState]}
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
