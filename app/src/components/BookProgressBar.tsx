import type { BookOperation } from "../bookProgress";

export function BookProgressBar({ operation }: { operation: BookOperation }) {
  const { label, starting, progress } = operation;
  const completed = progress?.completed ?? 0;
  const total = progress?.total ?? 0;
  const title = progress?.title;
  const fraction = progress?.file_fraction;
  const value = completed + (fraction ?? 0);
  const text = !progress
    ? starting
    : total > 0 && completed === total
      ? "Finishing…"
      : total > 0
        ? `${label} ${completed + 1} of ${total}${title ? ` · ${title}` : ""}…`
        : `${label}…`;
  return (
    <div className="book-operation">
      <span className="notice notice-info" role="status" title={text}>
        {text}
      </span>
      {fraction !== undefined && total > 0 ? (
        <div
          className="book-progress removal-progress"
          role="progressbar"
          aria-label={label}
          aria-valuemin={0}
          aria-valuemax={total}
          aria-valuenow={value}
        >
          <span style={{ transform: `scaleX(${value / total})` }} />
        </div>
      ) : (
        <progress
          className="book-progress"
          aria-label={label}
          max={total || 1}
          value={completed > 0 ? completed : undefined}
        />
      )}
    </div>
  );
}
