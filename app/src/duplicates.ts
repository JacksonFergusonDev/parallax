// Turns duplicate groups from the backend into what the review shows.
import type {
  Book,
  DeviceBook,
  DeviceDuplicateGroup,
  LibraryDuplicateGroup,
} from "./api";
import { shortDate, statusLabel } from "./device";
import type { ReviewGroup, ReviewMember } from "./components/DuplicateReview";

export function formatSize(bytes: number | null): string | null {
  if (bytes === null) return null;
  if (bytes < 1024 * 1024) return `${Math.max(1, Math.round(bytes / 1024))} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

const seriesLine = (series: string | null, index: string | null) =>
  series ? `${series}${index ? ` #${index}` : ""}` : null;

/** Groups with fewer than two known books (stale data) are dropped. */
const complete = (g: ReviewGroup | null): g is ReviewGroup =>
  g !== null && g.members.length > 1;

export function libraryReviewGroups(
  groups: LibraryDuplicateGroup[],
  books: Map<number, Book>,
  onKobo: (id: number) => boolean,
  cover: (b: Book) => string | null,
): ReviewGroup[] {
  return groups
    .map((g) => ({
      key: g.key,
      reasons: g.reasons,
      members: g.members.flatMap((m): ReviewMember[] => {
        const b = books.get(m.book_id);
        if (!b) return [];
        const facts = [
          seriesLine(b.series, b.series_index),
          [
            b.original_name.split(".").pop()?.toUpperCase(),
            formatSize(b.file_size),
          ]
            .filter(Boolean)
            .join(" · "),
          `Added ${shortDate(b.added_at)}`,
          b.tags.length ? b.tags.join(", ") : null,
          onKobo(b.id) ? "On Kobo" : null,
        ].filter((f): f is string => !!f);
        return [
          {
            id: String(b.id),
            title: b.title,
            subtitle: b.authors.join(", ") || "Unknown author",
            cover: cover(b),
            facts,
            remove: m.remove,
          },
        ];
      }),
    }))
    .filter(complete);
}

export function deviceReviewGroups(
  groups: DeviceDuplicateGroup[],
  books: Map<string, DeviceBook>,
  cover: (b: DeviceBook) => string | null,
): ReviewGroup[] {
  return groups
    .map((g) => ({
      key: g.key,
      reasons: g.reasons,
      members: g.members.flatMap((m): ReviewMember[] => {
        const b = books.get(m.content_id);
        if (!b) return [];
        const facts = [
          [m.format, formatSize(m.size)].filter(Boolean).join(" · "),
          seriesLine(b.series, b.series_number),
          b.read_status === "unread"
            ? "Unread"
            : `${statusLabel(b)} · last read ${shortDate(b.last_read)}`,
          b.path,
        ].filter((f): f is string => !!f);
        return [
          {
            id: m.content_id,
            title: b.title,
            subtitle: b.author ?? "Unknown author",
            cover: cover(b),
            facts,
            locked: m.removable
              ? undefined
              : "Bought from the Kobo store. Remove it on the Kobo if you don't need it.",
            remove: m.remove,
          },
        ];
      }),
    }))
    .filter(complete);
}
