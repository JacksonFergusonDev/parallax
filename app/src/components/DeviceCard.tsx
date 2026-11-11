import { memo } from "react";
import { GeneratedCover } from "./GeneratedCover";
import type { DeviceBook } from "../api";
import { statusLabel } from "../device";
import { deviceStateLabel } from "../library";

interface Props {
  book: DeviceBook;
  /** Cover read from the device, with a library fallback. */
  cover: string | null;
  pending?: "pending_import" | "updated";
  selected: boolean;
  onSelect: (id: string, event: React.MouseEvent) => void;
}

export const DeviceCard = memo(function DeviceCard({
  book,
  cover,
  pending,
  selected,
  onSelect,
}: Props) {
  return (
    <button
      className={`card ${selected ? "selected" : ""}`}
      aria-pressed={selected}
      aria-label={`${book.title}${book.author ? ` by ${book.author}` : ""}`}
      aria-description={pending && deviceStateLabel[pending]}
      onClick={(e) => onSelect(book.id, e)}
    >
      <div className="cover">
        {cover ? (
          <img
            src={cover}
            alt=""
            loading="eager"
            decoding="sync"
            draggable={false}
          />
        ) : (
          <GeneratedCover title={book.title} author={book.author} />
        )}
        {book.read_status !== "unread" && (
          <span className={`read-badge ${book.read_status}`}>
            {statusLabel(book)}
          </span>
        )}
        {pending && (
          <span className={`on-device ${pending}`}>
            {deviceStateLabel[pending]}
          </span>
        )}
      </div>
      <div className="card-title">{book.title}</div>
      <div className="card-author">{book.author ?? "Unknown author"}</div>
    </button>
  );
});
