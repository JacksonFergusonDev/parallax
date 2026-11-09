import { GeneratedCover } from "./GeneratedCover";
import { memo } from "react";
import { coverSrc, type Book } from "../api";
import { shortDate } from "../device";
import {
  progressLabel,
  sendStatusLabel,
  type DeviceState,
  type SendStatus,
} from "../library";

interface Props {
  book: Book;
  selected: boolean;
  deviceState: DeviceState;
  status: SendStatus | undefined;
  onSelect: (id: number, event: React.MouseEvent) => void;
}

export const LibraryRow = memo(function LibraryRow({
  book,
  selected,
  deviceState,
  status,
  onSelect,
}: Props) {
  const src = coverSrc(book);
  return (
    <button
      className={`list-row ${selected ? "selected" : ""}`}
      aria-pressed={selected}
      onClick={(e) => onSelect(book.id, e)}
      title={status?.stage === "failed" ? status.error : undefined}
    >
      <span className="thumb">
        {src ? (
          <img
            src={src}
            alt=""
            loading="lazy"
            decoding="async"
            draggable={false}
          />
        ) : (
          <GeneratedCover title={book.title} size="mini" />
        )}
      </span>
      <span className="cell">{book.title}</span>
      <span className="cell muted">
        {book.authors.join(", ") || "Unknown author"}
      </span>
      <span className="cell muted">
        {book.series
          ? `${book.series}${book.series_index ? ` #${book.series_index}` : ""}`
          : ""}
      </span>
      <span className="cell muted">{book.tags.join(", ")}</span>
      <span
        className="cell end muted"
        title={
          book.pages_estimated ? "Estimated from the word count" : undefined
        }
      >
        {book.pages?.toLocaleString()}
      </span>
      <span
        className={`cell end status-${book.reading?.status ?? "none"}`}
        title={
          book.reading?.last_read
            ? `Last read ${shortDate(book.reading.last_read)}`
            : undefined
        }
      >
        {progressLabel(book)}
      </span>
      {status ? (
        <span className={`cell end send-${status.stage}`}>
          {sendStatusLabel(status)}
        </span>
      ) : (
        <span className={`cell end kobo-${deviceState ?? "none"}`}>
          {deviceState === "stale" ? "Update" : deviceState ? "On Kobo" : ""}
        </span>
      )}
      <span className="cell end muted">{shortDate(book.added_at)}</span>
    </button>
  );
});
