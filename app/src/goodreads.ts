// What the Goodreads import says about a plan or a finished import. Pure, so
// it's testable without the dialog.
import type { GoodreadsPlan, GoodreadsReport } from "./api";

export const plural = (n: number, word: string) =>
  `${n} ${word}${n === 1 ? "" : "s"}`;

/** Joins "a", "b" and "c", leaving out nothing-to-say parts. */
const sentence = (parts: string[]) =>
  parts.length === 0
    ? "Nothing to change"
    : parts.length === 1
      ? parts[0]
      : `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;

/** For a plan ("will") or a report (done). Only says what happened or will. */
export function goodreadsSummary(
  result: GoodreadsPlan | GoodreadsReport,
): string {
  const done = "kept" in result;
  const will = done ? "" : "will be ";
  const parts: string[] = [];
  if (result.added > 0)
    parts.push(`${plural(result.added, "book")} ${will}added`);
  if (result.updated > 0)
    parts.push(`${plural(result.updated, "book")} ${will}updated`);
  if (done) {
    if (result.kept > 0)
      parts.push(`${plural(result.kept, "book")} kept as your own`);
    if (result.removed > 0)
      parts.push(`${plural(result.removed, "book")} removed`);
  }
  if (result.unchanged > 0)
    parts.push(`${result.unchanged} already up to date`);
  const text = `${sentence(parts)}.`;
  return result.skipped > 0
    ? `${text} ${plural(result.skipped, "row")} skipped because Goodreads gave no book id.`
    : text;
}
