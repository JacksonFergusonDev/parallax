import { memo } from "react";
import { coverArt } from "../coverArt";

interface Props {
  title: string;
  author?: string | null;
  /** Smaller type for thumbnails and mini covers. */
  size?: "full" | "mini";
}

/** A typeset stand-in cover for a book that has none. */
export const GeneratedCover = memo(function GeneratedCover({
  title,
  author,
  size = "full",
}: Props) {
  const { palette, motif } = coverArt(title);
  return (
    <div
      className={`gen-cover motif-${motif} ${size}`}
      style={
        {
          "--gc-bg": palette.bg,
          "--gc-ink": palette.ink,
          "--gc-accent": palette.accent,
        } as React.CSSProperties
      }
      aria-hidden="true"
    >
      <span className="gen-motif" />
      <span className="gen-title">{title}</span>
      {author && size === "full" && (
        <span className="gen-author">{author}</span>
      )}
    </div>
  );
});
