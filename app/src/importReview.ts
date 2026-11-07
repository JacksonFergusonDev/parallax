// What the review after adding books says and offers. Pure, apart from
// looking for covers, so it's testable.
import {
  coverInfo,
  searchCovers,
  type CoverCandidate,
  type ImportResult,
  type Issue,
  type Proposal,
  type Review,
} from "./api";
import { coverDimensions } from "./readCache";
import { withChanges } from "./enhance";

/** Covers shorter than this look soft on a Kobo screen (1680 px tall). */
const SHARP_HEIGHT = 1200;

export interface FailedFile {
  name: string;
  reason: string;
}

/** Books added together, and what was found about them. */
export interface ReviewBatch {
  ids: number[];
  failed: FailedFile[];
  review: Review;
}

export type Lookup =
  | { kind: "running"; done: number; total: number; title: string }
  | { kind: "done"; proposals: Proposal[]; problems: string[] }
  | { kind: "failed"; message: string };

export type Step = "duplicates" | "metadata" | "covers";

export const STEP_NAMES: Record<Step, string> = {
  duplicates: "Duplicates",
  metadata: "Book details",
  covers: "Covers",
};

export const plural = (n: number, word: string) =>
  `${n} ${word}${n === 1 ? "" : "s"}`;

/** Books this import actually added; ones already there aren't new. */
export const addedIds = (results: ImportResult[]) =>
  results.flatMap((r) =>
    "Ok" in r.outcome && r.outcome.Ok.kind === "added"
      ? [r.outcome.Ok.book.id]
      : [],
  );

export const failedFiles = (results: ImportResult[]): FailedFile[] =>
  results.flatMap((r) =>
    "Err" in r.outcome
      ? [
          {
            name: r.path.split(/[/\\]/).pop() ?? r.path,
            reason: r.outcome.Err,
          },
        ]
      : [],
  );

const withIssue = (
  review: Review,
  removed: Set<number>,
  ...issues: Issue[]
) => [
  ...new Set(
    review.findings
      .filter((f) => issues.includes(f.issue) && !removed.has(f.book_id))
      .map((f) => f.book_id),
  ),
];

/** Books with a low-resolution cover or none, to look for better ones. */
export const coverIds = (review: Review, removed = new Set<number>()) =>
  withIssue(review, removed, "no_cover", "small_cover");

export const lacksGenre = (review: Review, removed = new Set<number>()) =>
  withIssue(review, removed, "no_genre");

/** Worth opening straight away. A low-resolution cover alone isn't: that
 *  waits to see whether there's a better one to offer. */
export const needsReview = ({ failed, review }: ReviewBatch) =>
  failed.length > 0 ||
  review.duplicates.length > 0 ||
  review.findings.some((f) => f.issue !== "small_cover");

/** Books the review has something to say about. */
export const flaggedCount = ({ review }: ReviewBatch) =>
  new Set([
    ...review.findings.map((f) => f.book_id),
    ...review.duplicates.flatMap((g) => g.members.map((m) => m.book_id)),
  ]).size;

/** A proposal for a book that has a genre only adds noise: keep its series. */
export const reviewProposals = (
  proposals: Proposal[],
  review: Review,
  removed = new Set<number>(),
) => {
  const missing = new Set(lacksGenre(review));
  return withChanges(
    proposals
      .filter((p) => !removed.has(p.book_id))
      .map((p) => (missing.has(p.book_id) ? p : { ...p, genres: [] })),
  );
};

export interface MeasuredCover {
  candidate: CoverCandidate;
  width: number;
  height: number;
}

/** The tallest cover, if it's a real improvement on `current`'s height. */
export function bestCover(
  covers: MeasuredCover[],
  current: number | null,
): MeasuredCover | null {
  const tallest = [...covers].sort((a, b) => b.height - a.height)[0] ?? null;
  if (!tallest || current === null) return tallest;
  return tallest.height >= SHARP_HEIGHT && tallest.height > current
    ? tallest
    : null;
}

export interface CoverSearch {
  /** The book's cover height; null when it has none. */
  current: number | null;
  best: MeasuredCover | null;
}

/** Looks online for a better cover than the book's. */
export async function findBetterCover(id: number): Promise<CoverSearch> {
  const [info, candidates] = await Promise.all([
    coverInfo(id).catch(() => null),
    searchCovers(id).catch((): CoverCandidate[] => []),
  ]);
  const measured = await Promise.all(
    candidates.map((candidate) =>
      coverDimensions(candidate.url)
        .then(([width, height]) => ({ candidate, width, height }))
        .catch(() => null),
    ),
  );
  const current = info?.height ?? null;
  return {
    current,
    best: bestCover(
      measured.filter((m): m is MeasuredCover => m !== null),
      current,
    ),
  };
}

/** Whether a better cover exists for any flagged book. */
export async function anyBetterCover(review: Review) {
  for (const id of coverIds(review)) {
    if ((await findBetterCover(id)).best) return true;
  }
  return false;
}

/** Books whose details need typing in: nothing online can supply these. */
export function manualIds(
  review: Review,
  lookup: Lookup,
  removed = new Set<number>(),
) {
  const named = withIssue(review, removed, "no_author", "title_from_file");
  const proposed =
    lookup.kind === "done"
      ? new Set(
          lookup.proposals
            .filter((p) => p.genres.length > 0)
            .map((p) => p.book_id),
        )
      : new Set<number>();
  const genreless =
    lookup.kind === "running"
      ? []
      : lacksGenre(review, removed).filter((id) => !proposed.has(id));
  return [...new Set([...named, ...genreless])];
}

/** The steps that have something to decide, in order. */
export function reviewSteps(
  batch: ReviewBatch,
  lookup: Lookup,
  removed: Set<number>,
  /** Whether there's a cover to offer or upscale; null while searching. */
  coversToOffer: boolean | null,
): Step[] {
  const { review } = batch;
  const steps: Step[] = [];
  if (review.duplicates.length) steps.push("duplicates");
  const proposals =
    lookup.kind === "done"
      ? reviewProposals(lookup.proposals, review, removed)
      : [];
  if (
    lookup.kind === "running" ||
    proposals.length > 0 ||
    manualIds(review, lookup, removed).length > 0
  )
    steps.push("metadata");
  if (coverIds(review, removed).length > 0 && coversToOffer !== false)
    steps.push("covers");
  return steps;
}

export interface SummaryLine {
  key: string;
  text: string;
  advice: string;
  /** Can't be fixed here, so it's worth reading. */
  warning?: boolean;
  /** Books the user may want to remove instead. */
  removable?: number[];
}

const count = (n: number, one: string, many: string) =>
  `${n} ${n === 1 ? one : many}`;

/** What was found, by what to do about it. */
export function summaryLines(
  batch: ReviewBatch,
  lookup: Lookup,
  removed: Set<number>,
  /** Books with a better cover so far; null while searching. */
  betterCovers: number | null,
  titles: (id: number) => string,
): SummaryLine[] {
  const { failed, review } = batch;
  const lines: SummaryLine[] = [];
  if (failed.length)
    lines.push({
      key: "failed",
      text: `${count(failed.length, "file", "files")} couldn't be added`,
      advice: failed.map((f) => `${f.name}: ${f.reason}`).join(" · "),
      warning: true,
    });
  const damaged = withIssue(review, removed, "damaged");
  if (damaged.length)
    lines.push({
      key: "damaged",
      text: `${count(damaged.length, "book is", "books are")} damaged`,
      advice: `${damaged.map(titles).join(", ")} may not open properly. Another copy is the only fix.`,
      warning: true,
      removable: damaged,
    });
  const drm = withIssue(review, removed, "drm");
  if (drm.length)
    lines.push({
      key: "drm",
      text: `${count(drm.length, "book has", "books have")} DRM`,
      advice: `${drm.map(titles).join(", ")} may not open on your Kobo, and the app can't change that.`,
      warning: true,
      removable: drm,
    });
  if (review.duplicates.length)
    lines.push({
      key: "duplicates",
      text: `${count(review.duplicates.length, "possible duplicate", "possible duplicates")}`,
      advice: "Compare the copies and choose what to keep.",
    });
  const genreless = lacksGenre(review, removed);
  const proposals =
    lookup.kind === "done"
      ? reviewProposals(lookup.proposals, review, removed)
      : [];
  if (genreless.length) {
    const found = proposals.filter((p) => p.genres.length > 0).length;
    lines.push({
      key: "genres",
      text: `${count(genreless.length, "book is", "books are")} missing a genre`,
      advice:
        lookup.kind === "running"
          ? `Looking online${lookup.total ? ` (${lookup.done} of ${lookup.total})` : ""}…`
          : found
            ? `Found genres for ${found}. You'll check them before they're saved.`
            : "Nothing found online, but you can add them yourself.",
    });
  }
  const series = proposals.filter((p) => p.series !== null).length;
  if (series)
    lines.push({
      key: "series",
      text: `Found a series for ${plural(series, "book")}`,
      advice: "You'll check it before it's saved.",
    });
  const authorless = withIssue(review, removed, "no_author").length;
  if (authorless)
    lines.push({
      key: "author",
      text: `${count(authorless, "book has", "books have")} no author`,
      advice: "You can add it in the next steps.",
    });
  const untitled = withIssue(review, removed, "title_from_file").length;
  if (untitled)
    lines.push({
      key: "title",
      text: `${count(untitled, "title looks", "titles look")} like a file name`,
      advice: "You can correct it in the next steps.",
    });
  const covers = coverIds(review, removed).length;
  if (covers)
    lines.push({
      key: "covers",
      text: `${count(covers, "book needs", "books need")} a better cover`,
      advice:
        betterCovers === null
          ? "Looking for better ones…"
          : betterCovers
            ? `Found better covers for ${betterCovers}.`
            : "No better covers found online.",
    });
  return lines;
}
