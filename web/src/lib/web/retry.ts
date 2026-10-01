export interface RetryPolicy {
  attempts: number;
  baseMs: number;
  capMs: number;
}

export const TRACK_POLICY: RetryPolicy = { attempts: 3, baseMs: 1000, capMs: 30_000 };
export const SEGMENT_POLICY: RetryPolicy = { attempts: 3, baseMs: 500, capMs: 5000 };

export function backoffDelay(attempt: number, policy: RetryPolicy, random: () => number = Math.random): number {
  const exponent = Math.min(Math.max(attempt - 1, 0), 20);
  const ceiling = Math.min(policy.baseMs * 2 ** exponent, policy.capMs);
  const half = ceiling / 2;
  return half + half * random();
}

function abortError(): DOMException {
  return new DOMException("Cancelado", "AbortError");
}

export function isAbort(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

export function sleep(ms: number, signal: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal.aborted) {
      reject(abortError());
      return;
    }
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", onAbort);
      resolve();
    }, ms);
    const onAbort = (): void => {
      clearTimeout(timer);
      reject(abortError());
    };
    signal.addEventListener("abort", onAbort, { once: true });
  });
}
