import type { BatchTransferProgress } from "../transferEstimate";
import { sendStatusLabel } from "../library";
import { timeLeft } from "../upscaleEstimate";

export function BatchTransferBar({
  progress,
}: {
  progress: BatchTransferProgress;
}) {
  const { completed, total, value, secondsLeft, activeComic } = progress;
  return (
    <div className="book-operation batch-transfer">
      <div className="batch-transfer-summary">
        <span role="status">
          {completed} of {total} books finished
        </span>
        <span className="muted">{timeLeft(secondsLeft)}</span>
      </div>
      <progress
        className="book-progress"
        aria-label="Kobo transfer batch"
        max={total}
        value={value}
      />
      {activeComic && (
        <div className="book-operation batch-transfer-comic">
          <div className="batch-transfer-summary muted">
            <span className="batch-transfer-title" title={activeComic.file}>
              {activeComic.file}
            </span>
            <span>{sendStatusLabel(activeComic)}</span>
          </div>
          <progress
            className="book-progress copy-progress"
            aria-label="Comic pages processed"
            max={activeComic.pages}
            value={activeComic.page}
          />
        </div>
      )}
    </div>
  );
}
