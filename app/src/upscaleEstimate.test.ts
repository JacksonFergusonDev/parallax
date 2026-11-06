import { describe, expect, it } from "vitest";
import { UpscaleEstimate, timeLeft } from "./upscaleEstimate";

describe("UpscaleEstimate", () => {
  it("has no estimate until a tile has been timed", () => {
    const run = new UpscaleEstimate(10);
    const first = run.record({ id: 1, done: 1, total: 10 }, 60_000);
    expect(first.secondsLeft).toBeNull();
    expect(first.target).toBeCloseTo(0.2);
  });

  it("leaves the model's load time out of the estimate", () => {
    const run = new UpscaleEstimate(10);
    run.record({ id: 1, done: 1, total: 10 }, 60_000);
    const second = run.record({ id: 1, done: 2, total: 10 }, 62_000);
    expect(second.tileMs).toBe(2000);
    expect(second.secondsLeft).toBe(16);
  });

  it("counts every cover in the queue", () => {
    const run = new UpscaleEstimate(10);
    run.record({ id: 1, done: 3, total: 4 }, 0);
    run.record({ id: 1, done: 4, total: 4 }, 1000);
    const next = run.record({ id: 2, done: 1, total: 6 }, 2000);
    expect(next.secondsLeft).toBe(5);
    expect(next.target).toBeCloseTo(0.6);
  });

  it("follows recent tiles as the Mac slows down", () => {
    const run = new UpscaleEstimate(100);
    let now = 0;
    for (let done = 1; done <= 20; done++) {
      now += done <= 10 ? 1000 : 2000;
      run.record({ id: 1, done, total: 100 }, now);
    }
    const latest = run.record({ id: 1, done: 21, total: 100 }, now + 2000);
    expect(latest.tileMs).toBe(2000);
  });

  it("never heads past the end", () => {
    const run = new UpscaleEstimate(2);
    run.record({ id: 1, done: 1, total: 2 }, 0);
    const last = run.record({ id: 1, done: 2, total: 2 }, 1000);
    expect(last.target).toBe(1);
    expect(last.secondsLeft).toBe(0);
  });

  it("copes with a queue larger than planned", () => {
    const run = new UpscaleEstimate(0);
    expect(run.record({ id: 1, done: 1, total: 4 }, 0).target).toBe(0.5);
  });
});

describe("timeLeft", () => {
  it.each([
    [null, "Estimating time left…"],
    [0, "Finishing…"],
    [7, "About 10s left"],
    [50, "About 50s left"],
    [70, "About a minute left"],
    [200, "About 3 min left"],
  ])("%s", (seconds, text) => expect(timeLeft(seconds)).toBe(text));
});
