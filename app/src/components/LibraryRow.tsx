import { TransferProgress } from "./TransferProgress";
import { GeneratedCover } from "./GeneratedCover";
import { memo } from "react";
import { coverSrc, type Book } from "../api";
import { shortDate } from "../device";
import {
  deviceStateLabel,
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
            loading="eager"
            decoding="sync"
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
      <span className="cell muted" data-column="series">
        {book.series
          ? `${book.series}${book.series_index ? ` #${book.series_index}` : ""}`
          : ""}
      </span>
      <span className="cell muted" data-column="tags">
        {book.tags.join(", ")}
      </span>
      <span
        className="cell end muted"
        data-column="pages"
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
          {status.stage === "copying" && <TransferProgress status={status} />}
        </span>
      ) : (
        <span className={`cell end kobo-status kobo-${deviceState ?? "none"}`}>
          {deviceState && deviceStateLabel[deviceState]}
        </span>
      )}
      <span className="cell end muted" data-column="added">
        {shortDate(book.added_at)}
      </span>
    </button>
  );
});
