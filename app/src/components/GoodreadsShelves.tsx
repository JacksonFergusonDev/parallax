import { useState } from "react";
import { goodreadsEdit, type Book, type GoodreadsEdit } from "../api";

const STATUSES = [
  ["to_read", "Want to read"],
  ["currently_reading", "Reading"],
  ["read", "Read"],
] as const;

/** Goodreads' own shelves, which are statuses rather than shelves you add. */
const STATUS_SHELVES = new Set(["read", "to-read", "currently-reading"]);

/**
 * Your Goodreads status, rating and shelves for a library book. Changes save
 * at once and are kept here; the next import asks before overwriting one.
 */
export function GoodreadsShelves({
  book,
  onChanged,
}: {
  book: Book;
  onChanged: () => void;
}) {
  const match = book.goodreads;
  const [shelf, setShelf] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  if (!match) return null;

  const apply = async (edit: GoodreadsEdit) => {
    setBusy(true);
    setError(null);
    try {
      await goodreadsEdit(match.goodreads_id, edit);
      onChanged();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };
  const addShelf = () => {
    const name = shelf.trim().toLowerCase().replace(/\s+/g, "-");
    if (!name) return;
    setShelf("");
    if (STATUS_SHELVES.has(name)) {
      setError(
        `"${name}" is a Goodreads status. Choose it above to change the book's status.`,
      );
      return;
    }
    if (!match.shelves.includes(name)) void apply({ add_shelves: [name] });
  };

  return (
    <section className="goodreads-shelves" aria-label="Goodreads">
      <h3>Goodreads</h3>
      <div className="goodreads-choice" role="radiogroup" aria-label="Status">
        {STATUSES.map(([value, label]) => (
          <button
            key={value}
            type="button"
            role="radio"
            className="secondary"
            aria-checked={match.shelf === value}
            disabled={busy}
            onClick={() => match.shelf !== value && apply({ shelf: value })}
          >
            {label}
          </button>
        ))}
      </div>
      <label className="goodreads-rating">
        Rating
        <select
          value={match.rating ?? 0}
          disabled={busy}
          onChange={(e) => apply({ rating: Number(e.target.value) })}
        >
          <option value={0}>Not rated</option>
          {[1, 2, 3, 4, 5].map((n) => (
            <option key={n} value={n}>
              {n} {n === 1 ? "star" : "stars"}
            </option>
          ))}
        </select>
      </label>
      <div className="goodreads-tags">
        {match.shelves.map((name) => (
          <span key={name} className="goodreads-tag">
            {name}
            <button
              type="button"
              className="link"
              aria-label={`Remove from ${name}`}
              disabled={busy}
              onClick={() => apply({ remove_shelves: [name] })}
            >
              ×
            </button>
          </span>
        ))}
        <input
          value={shelf}
          placeholder="Add to a shelf"
          aria-label="Add to a shelf"
          disabled={busy}
          onChange={(e) => setShelf(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && addShelf()}
          onBlur={addShelf}
        />
      </div>
      {error && <p className="notice notice-error wrap">{error}</p>}
    </section>
  );
}
