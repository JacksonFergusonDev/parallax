// Labels and grouping for "Check library" findings.
import type { CheckKind, Finding } from "./api";

export interface CheckInfo {
  label: string;
  /** What it means and how to fix it. */
  help: string;
  /** Notes are informational; problems are worth fixing. */
  note?: boolean;
}

export const CHECKS: Record<CheckKind, CheckInfo> = {
  unreadable: {
    label: "Can't be read",
    help: "The file is damaged. Re-import it from another copy.",
  },
  drm: {
    label: "Protected by DRM",
    help: "These won't open on a Kobo when sent as files.",
  },
  missing_files: {
    label: "Files missing from the book",
    help: "The book lists files it doesn't contain; some pages or images may be blank.",
  },
  broken_links: {
    label: "Broken links or images",
    help: "Links or pictures point at files that aren't in the book.",
  },
  no_cover: {
    label: "No cover",
    help: "Select the book and choose “Find cover…”.",
  },
  small_cover: {
    label: "Low-resolution cover",
    help: "Shorter than 1200 pixels, so it looks soft on a Kobo. Select the book and choose “Find cover…”.",
  },
  no_author: {
    label: "No author",
    help: "Select the book to add one.",
  },
  series_without_number: {
    label: "Series without a number",
    help: "Select the book to add its number, or use “Find metadata…”.",
  },
  series_gap: {
    label: "Gaps in series",
    help: "Your library skips a number in these series.",
    note: true,
  },
  unmanifested: {
    label: "Extra files inside the book",
    help: "Files the book doesn't list. Harmless, but they take space.",
    note: true,
  },
  clutter: {
    label: "Mac or Windows clutter",
    help: "Files like .DS_Store. Left out automatically when books are sent.",
    note: true,
  },
  damaged_pages: {
    label: "Damaged pages",
    help: "Pages in these comics aren't readable images. Re-import them from another copy.",
  },
  embedded_fonts: {
    label: "Embedded fonts",
    help: "Not a problem: the Kobo can use the publisher's fonts.",
    note: true,
  },
};

const ORDER = Object.keys(CHECKS) as CheckKind[];

export interface CheckGroup {
  kind: CheckKind;
  info: CheckInfo;
  findings: Finding[];
}

/** Problems first, then notes, each in a fixed order. */
export function groupFindings(findings: Finding[]): CheckGroup[] {
  const groups = new Map<CheckKind, Finding[]>();
  for (const f of findings)
    groups.set(f.check, [...(groups.get(f.check) ?? []), f]);
  return [...groups.entries()]
    .map(([kind, list]) => ({ kind, info: CHECKS[kind], findings: list }))
    .sort(
      (a, b) =>
        Number(!!a.info.note) - Number(!!b.info.note) ||
        ORDER.indexOf(a.kind) - ORDER.indexOf(b.kind),
    );
}
