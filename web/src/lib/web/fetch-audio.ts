import type { Bytes } from "./tags";
import { SEGMENT_POLICY, backoffDelay, isAbort, sleep } from "./retry";

const MAX_AUDIO_BYTES = 1024 ** 3;
const MAX_ARTWORK_BYTES = 20 * 1024 ** 2;
const HLS_CONCURRENCY = 6;
const TRUSTED_SUFFIXES = ["sndcdn.com", "soundcloud.com", "soundcloud.cloud"];

export class TransferError extends Error {
  override name = "TransferError";
}

export type Progress = (bytes: number, fraction: number | null) => void;

export function isTrustedMediaUrl(raw: string): boolean {
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return false;
  }
  if (url.protocol !== "https:" || url.username || url.password) return false;
  const host = url.hostname.toLowerCase();
  return TRUSTED_SUFFIXES.some((suffix) => host === suffix || host.endsWith(`.${suffix}`));
}

async function request(url: string, signal: AbortSignal): Promise<Response> {
  if (!isTrustedMediaUrl(url)) throw new TransferError("URL de midia fora da allowlist");
  let response: Response;
  try {
    response = await fetch(url, { signal, credentials: "omit", referrerPolicy: "no-referrer", cache: "no-store" });
  } catch (error) {
    if (isAbort(error)) throw error;
    throw new TransferError("falha de rede");
  }
  if (!response.ok) throw new TransferError(`HTTP ${String(response.status)}`);
  return response;
}

function join(chunks: readonly Uint8Array[], total: number): Bytes {
  const out = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
}

async function readBody(response: Response, limit: number, onChunk?: (received: number) => void): Promise<Bytes> {
  const declared = Number(response.headers.get("content-length"));
  const expected = Number.isFinite(declared) && declared > 0 ? declared : null;
  if (expected !== null && expected > limit) throw new TransferError("arquivo maior que o limite");
  const reader = response.body?.getReader();
  if (!reader) return new Uint8Array(await response.arrayBuffer());
  const chunks: Uint8Array[] = [];
  let received = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    received += value.length;
    if (received > limit) {
      await reader.cancel();
      throw new TransferError("arquivo maior que o limite");
    }
    chunks.push(value);
    onChunk?.(received);
  }
  if (expected !== null && received !== expected) throw new TransferError("download incompleto");
  return join(chunks, received);
}

export async function fetchProgressive(url: string, signal: AbortSignal, onProgress: Progress): Promise<Bytes[]> {
  const response = await request(url, signal);
  const declared = Number(response.headers.get("content-length"));
  const total = Number.isFinite(declared) && declared > 0 ? declared : null;
  const body = await readBody(response, MAX_AUDIO_BYTES, (received) =>
    onProgress(received, total ? Math.min(received / total, 1) : null),
  );
  return [body];
}

async function fetchPart(url: string, signal: AbortSignal): Promise<Bytes> {
  for (let attempt = 1; ; attempt += 1) {
    try {
      return await readBody(await request(url, signal), MAX_AUDIO_BYTES);
    } catch (error) {
      if (isAbort(error) || attempt >= SEGMENT_POLICY.attempts) throw error;
      await sleep(backoffDelay(attempt, SEGMENT_POLICY), signal);
    }
  }
}

export async function fetchHls(parts: readonly string[], signal: AbortSignal, onProgress: Progress): Promise<Bytes[]> {
  if (parts.length === 0) throw new TransferError("manifesto HLS vazio");
  const results: Bytes[] = new Array<Bytes>(parts.length);
  let next = 0;
  let done = 0;
  let bytes = 0;
  const local = new AbortController();
  const stop = (): void => local.abort();
  signal.addEventListener("abort", stop, { once: true });
  const worker = async (): Promise<void> => {
    while (next < parts.length) {
      const index = next;
      next += 1;
      const part = await fetchPart(parts[index] ?? "", local.signal);
      results[index] = part;
      done += 1;
      bytes += part.length;
      if (bytes > MAX_AUDIO_BYTES) throw new TransferError("arquivo maior que o limite");
      onProgress(bytes, done / parts.length);
    }
  };
  try {
    await Promise.all(
      Array.from({ length: Math.min(HLS_CONCURRENCY, parts.length) }, () =>
        worker().catch((error: unknown) => {
          local.abort();
          throw error;
        }),
      ),
    );
  } catch (error) {
    if (signal.aborted) throw signal.reason instanceof DOMException ? signal.reason : error;
    throw error;
  } finally {
    signal.removeEventListener("abort", stop);
  }
  return results;
}

export async function fetchArtwork(urls: readonly (string | null)[], signal: AbortSignal): Promise<Bytes | null> {
  for (const url of urls) {
    if (!url) continue;
    try {
      return await readBody(await request(url, signal), MAX_ARTWORK_BYTES);
    } catch (error) {
      if (isAbort(error)) throw error;
    }
  }
  return null;
}
