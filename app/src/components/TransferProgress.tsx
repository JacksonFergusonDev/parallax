import type { SendStatus } from "../library";

export function TransferProgress({
  status,
}: {
  status: Extract<SendStatus, { stage: "copying" }>;
}) {
  return (
    <progress
      className="book-progress copy-progress"
      aria-label="Transfer to Kobo"
      max={status.totalBytes || 1}
      value={status.totalBytes > 0 ? status.bytes : undefined}
    />
  );
}
