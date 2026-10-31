import { memo } from "react";
import type { DeviceBook } from "../api";
import { statusLabel } from "../device";

interface Props {
  book: DeviceBook;
  /** Cover from the library copy, when there is one. */
  cover: string | null;
  selected: boolean;
  onSelect: (id: string, event: React.MouseEvent) => void;
}

export const DeviceCard = memo(function DeviceCard({
  book,
  cover,
  selected,
  onSelect,
}: Props) {
  return (
    <button
      className={`card ${selected ? "selected" : ""}`}
      aria-pressed={selected}
      aria-label={`${book.title}${book.author ? ` by ${book.author}` : ""}`}
      onClick={(e) => onSelect(book.id, e)}
    >
      <div className="cover">
        {cover ? (
          <img src={cover} alt="" loading="lazy" draggable={false} />
        ) : (
          <div className="cover-placeholder">
            <span>{book.title}</span>
          </div>
        )}
        {book.read_status !== "unread" && (
          <span className={`read-badge ${book.read_status}`}>
            {statusLabel(book)}
          </span>
        )}
      </div>
      <div className="card-title">{book.title}</div>
      <div className="card-author">{book.author ?? "Unknown author"}</div>
    </button>
  );
});
