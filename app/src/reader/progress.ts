// The footer's sense of where the reader is: page numbers, time left, and the
// chapter marks on the slider.
import type { Location } from "foliate-js/view.js";

/** "4 min", "1 h 20 min". The engine's minutes are a rough estimate. */
export function duration(minutes: number): string {
  const rounded = Math.round(minutes);
  if (rounded < 1) return "less than a minute";
  if (rounded < 60) return `${rounded} min`;
  const hours = Math.floor(rounded / 60);
  const rest = rounded % 60;
  return rest ? `${hours} h ${rest} min` : `${hours} h`;
}

export interface Whereabouts {
  /** The print edition's page, when the book lists them. */
  page: string | null;
  chapterLeft: string | null;
  bookLeft: string | null;
}

export function whereabouts(location: Location): Whereabouts {
  const label = location.pageItem?.label?.trim();
  const { time } = location;
  const finite = (n: number | undefined): n is number =>
    typeof n === "number" && Number.isFinite(n);
  return {
    page: label ? `p. ${label}` : null,
    chapterLeft: finite(time?.section)
      ? `${duration(time.section)} left in chapter`
      : null,
    bookLeft: finite(time?.total)
      ? `About ${duration(time.total)} left in the book`
      : null,
  };
}

/**
 * Where chapters start along the slider, as fractions. `starts` is each
 * chapter's section index; `fractions` is where each section starts. Marks
 * too close to show apart, and the two ends, are left out.
 */
export function chapterMarks(starts: number[], fractions: number[]): number[] {
  const marks: number[] = [];
  for (const index of [...new Set(starts)].sort((a, b) => a - b)) {
    const at = fractions[index];
    if (at === undefined || at <= 0.005 || at >= 0.995) continue;
    if (marks.length && at - marks[marks.length - 1] < 0.01) continue;
    marks.push(at);
  }
  return marks;
}
