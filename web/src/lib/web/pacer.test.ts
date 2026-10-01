import { afterEach, describe, expect, it, vi } from "vitest";

import { API_PACING, Pacer } from "./pacer";
import { isAbort } from "./retry";

afterEach(() => {
  vi.useRealTimers();
});

describe("Pacer", () => {
  it("keeps the average rate under the server limit of 150 per minute", () => {
    const average = (API_PACING.minGapMs + API_PACING.maxGapMs) / 2;
    expect(60_000 / average).toBeLessThan(150);
    expect(60_000 / API_PACING.minGapMs).toBeLessThanOrEqual(150);
  });

  it("lets the first call through and spaces the next ones by a random gap", async () => {
    vi.useFakeTimers();
    let clock = 0;
    const randoms = [0, 1, 0.5];
    const pacer = new Pacer({ minGapMs: 400, maxGapMs: 900 }, () => clock, () => randoms.shift() ?? 0);
    const signal = new AbortController().signal;
    const started: number[] = [];
    const calls = [0, 1, 2].map(() => pacer.wait(signal).then(() => started.push(clock)));
    await vi.advanceTimersByTimeAsync(0);
    expect(started).toEqual([0]);
    clock = 400;
    await vi.advanceTimersByTimeAsync(400);
    expect(started).toEqual([0, 400]);
    clock = 1300;
    await vi.advanceTimersByTimeAsync(900);
    await Promise.all(calls);
    expect(started).toEqual([0, 400, 1300]);
  });

  it("does not wait again after an idle period", async () => {
    let clock = 0;
    const pacer = new Pacer({ minGapMs: 400, maxGapMs: 900 }, () => clock, () => 1);
    const signal = new AbortController().signal;
    await pacer.wait(signal);
    clock = 10_000;
    const spy = vi.spyOn(globalThis, "setTimeout");
    await pacer.wait(signal);
    expect(spy).not.toHaveBeenCalled();
  });

  it("stops waiting when the task is cancelled", async () => {
    const pacer = new Pacer({ minGapMs: 5000, maxGapMs: 5000 }, () => 0, () => 0);
    const controller = new AbortController();
    await pacer.wait(controller.signal);
    const waiting = pacer.wait(controller.signal);
    controller.abort();
    await expect(waiting).rejects.toSatisfy(isAbort);
  });
});
