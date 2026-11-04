import { describe, expect, it } from "vitest";
import type { StatBook } from "./api";
import { finishedByMonth, finishedByYear, hours, ticks } from "./stats";

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
