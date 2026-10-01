import { sleep } from "./retry";

export interface PacingPolicy {
  minGapMs: number;
  maxGapMs: number;
}

export const API_PACING: PacingPolicy = { minGapMs: 400, maxGapMs: 900 };

export class Pacer {
  private next = 0;

  constructor(
    private readonly policy: PacingPolicy = API_PACING,
    private readonly now: () => number = () => performance.now(),
    private readonly random: () => number = Math.random,
  ) {}

  async wait(signal: AbortSignal): Promise<void> {
    const now = this.now();
    const at = Math.max(now, this.next);
    const { minGapMs, maxGapMs } = this.policy;
    this.next = at + minGapMs + (maxGapMs - minGapMs) * this.random();
    if (at > now) await sleep(at - now, signal);
  }
}
