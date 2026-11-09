import { expect, it } from "vitest";
import { TransferEstimate } from "./transferEstimate";

it("advances through pages, USB copying, and the final outcome", () => {
  const estimate = new TransferEstimate(2);
  const first = { index: 1, total: 2, file: "First" };
  const comic = { ...first, stage: "processing_comic" as const, pages: 100 };
  const copy = { ...first, stage: "copying" as const, total_bytes: 100 };
  expect(estimate.initial()).toMatchObject({ value: 0, secondsLeft: null });
  expect(estimate.record({ ...comic, page: 0 }, 0).value).toBe(0);
  expect(estimate.record({ ...comic, page: 50 }, 2000)).toMatchObject({
    completed: 0,
    value: 0.4,
    secondsLeft: 8,
  });
  expect(estimate.record({ ...comic, page: 100 }, 4000).value).toBe(0.8);
  expect(estimate.record({ ...copy, bytes: 0 }, 4200).value).toBe(0.8);
  expect(estimate.record({ ...copy, bytes: 100 }, 4900).value).toBe(0.99);
  expect(
    estimate.record({ ...first, stage: "done", device_path: "first" }, 5000),
  ).toMatchObject({ completed: 1, value: 1, secondsLeft: 5 });
  const second = { ...first, index: 2, file: "Second" };
  expect(
    estimate.record({ ...copy, ...second, bytes: 50 }, 6000).value,
  ).toBeCloseTo(1.495);
  expect(
    estimate.record({ ...second, stage: "done", device_path: "second" }, 7000),
  ).toMatchObject({ completed: 2, value: 2, secondsLeft: 0 });
});

it("counts failures as finished without using them to predict later volumes", () => {
  const estimate = new TransferEstimate(3);
  const base = { index: 1, total: 3, file: "Volume" };
  estimate.record({ ...base, stage: "preparing" }, 0);
  expect(
    estimate.record({ ...base, stage: "failed", error: "Damaged" }, 100),
  ).toMatchObject({ completed: 1, value: 1, secondsLeft: null });
  const second = { ...base, index: 2 };
  estimate.record({ ...second, stage: "preparing" }, 100);
  expect(
    estimate.record({ ...second, stage: "done", device_path: "second" }, 6100),
  ).toMatchObject({ completed: 2, value: 2, secondsLeft: 6 });
  expect(
    estimate.record(
      { ...base, index: 3, stage: "failed", error: "No space" },
      6500,
    ),
  ).toMatchObject({ completed: 3, value: 3, secondsLeft: 0 });
});
