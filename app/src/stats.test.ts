import { describe, expect, it } from "vitest";
import type { StatBook } from "./api";
import {
  finishedByMonth,
  finishedByYear,
  hours,
  readingHeadline,
  spokenTime,
  startedBooks,
  ticks,
  titleKey,
} from "./stats";

const book = (finished_at: string | null): StatBook => ({
  content_id: finished_at ?? "x",
  title: "T",
  author: null,
  percent: 100,
  time_secs: null,
  left_secs: null,
  last_read: null,
  finished_at,
  times_finished: 1,
  on_device: false,
});

describe("reading stats", () => {
  const now = new Date(2026, 10, 4);

  it("counts finished books per year, filling empty years", () => {
    const years = finishedByYear(
      [
        book("2023-12-24T10:00:00Z"),
        book("2023-03-01T10:00:00Z"),
        book("2025-06-01T10:00:00Z"),
        book(null),
      ],
      now,
    );
    expect(years.map((y) => [y.label, y.count])).toEqual([
      ["2023", 2],
      ["2024", 0],
      ["2025", 1],
      ["2026", 0],
    ]);
    expect(finishedByYear([], now)).toEqual([]);
  });

  it("counts the last twelve months", () => {
    const months = finishedByMonth(
      [
        book("2026-11-02T10:00:00Z"),
        book("2025-12-15T10:00:00Z"),
        book("2025-11-15T10:00:00Z"),
      ],
      now,
    );
    expect(months).toHaveLength(12);
    expect(months[0].key).toBe("2025-12");
    expect(months[0].count).toBe(1);
    expect(months[11].key).toBe("2026-11");
    expect(months[11].count).toBe(1);
  });

  it("formats hours and axis ticks", () => {
    expect(hours(null)).toBe("—");
    expect(hours(600)).toBe("10 min");
    expect(hours(3.5 * 3600)).toBe("3.5 h");
    expect(hours(58_680)).toBe("16 h");
    expect(ticks(41)).toEqual([0, 20, 40, 60]);
    expect(ticks(4)).toEqual([0, 2, 4]);
    expect(ticks(0)).toEqual([0, 1]);
  });
});

describe("titleKey", () => {
  it("ignores case and punctuation", () => {
    expect(titleKey("I, Robot")).toBe(titleKey("i robot"));
    expect(titleKey("  Dune: Messiah ")).toBe("dune messiah");
  });
});

describe("spokenTime", () => {
  it("speaks minutes and hours", () => {
    expect(spokenTime(0)).toBeNull();
    expect(spokenTime(60)).toBe("1 minute");
    expect(spokenTime(40 * 60)).toBe("40 minutes");
    expect(spokenTime(3600)).toBe("1 hour");
    expect(spokenTime(16.2 * 3600)).toBe("16 hours");
  });
});

describe("readingHeadline", () => {
  const now = new Date("2026-06-01T12:00:00Z");
  it("leads with books finished this year", () => {
    expect(
      readingHeadline(
        {
          total_secs: 7200,
          finished: [
            book("2026-02-01T00:00:00Z"),
            book("2025-02-01T00:00:00Z"),
          ],
          reading: [],
        },
        now,
      ),
    ).toBe("1 book finished this year, and 2 hours spent reading.");
  });
  it("falls back to books in progress, then time", () => {
    const reading = { ...book(null), percent: 20 };
    expect(
      readingHeadline({ total_secs: 0, finished: [], reading: [reading] }, now),
    ).toBe("1 book on the go.");
    expect(
      readingHeadline({ total_secs: 600, finished: [], reading: [] }, now),
    ).toBe("10 minutes spent reading on your Kobo.");
    expect(
      readingHeadline({ total_secs: 0, finished: [], reading: [] }, now),
    ).toBe("Your reading, at a glance.");
  });
});

describe("startedBooks", () => {
  const b = (time_secs: number | null, percent: number): StatBook => ({
    ...book(null),
    content_id: `${time_secs}-${percent}`,
    time_secs,
    percent,
  });
  it("keeps books with half an hour or more of reading", () => {
    const kept = startedBooks([b(29 * 60, 40), b(30 * 60, 1), b(5 * 3600, 3)]);
    expect(kept.map((x) => x.time_secs)).toEqual([30 * 60, 5 * 3600]);
  });
  it("falls back to progress when no time was recorded", () => {
    expect(
      startedBooks([b(null, 2), b(null, 12)]).map((x) => x.percent),
    ).toEqual([12]);
  });
});
