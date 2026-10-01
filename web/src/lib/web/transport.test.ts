import { afterEach, describe, expect, it, vi } from "vitest";

import { TransferError, fetchArtwork, fetchHls, fetchProgressive, isTrustedMediaUrl } from "./fetch-audio";
import { HttpError, getJson, postJson } from "./http";
import { SEGMENT_POLICY, TRACK_POLICY, backoffDelay, isAbort, sleep } from "./retry";
import { DirectorySink, DownloadSink, safeFileName, saveArtifact } from "./sink";

const href = (input: RequestInfo | URL): string => (input instanceof Request ? input.url : input.toString());
const CDN = "https://cf-media.sndcdn.com";

function fetchMock(handler: (url: string, init?: RequestInit) => Response | Promise<Response>): ReturnType<typeof vi.fn> {
  const mock = vi.fn((input: RequestInfo | URL, init?: RequestInit) => Promise.resolve(handler(href(input), init)));
  vi.stubGlobal("fetch", mock);
  return mock;
}

async function zipText(blob: Blob | undefined): Promise<string> {
  return new TextDecoder("latin1").decode(await (blob ?? new Blob([])).arrayBuffer());
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("http", () => {
  it("sends JSON and parses the answer", async () => {
    const mock = fetchMock(() => Response.json({ ok: true }));
    await expect(getJson("/api/inspect", { url: "https://soundcloud.com/a b", mode: "track" })).resolves.toEqual({ ok: true });
    await expect(getJson("/api/config", {})).resolves.toEqual({ ok: true });
    await expect(postJson("/api/plan", { url: "u" })).resolves.toEqual({ ok: true });
    expect(mock.mock.calls[0]?.[0]).toBe("/api/inspect?url=https%3A%2F%2Fsoundcloud.com%2Fa+b&mode=track");
    expect(mock.mock.calls[1]?.[0]).toBe("/api/config");
    expect(mock.mock.calls[2]?.[1]).toMatchObject({ method: "POST", body: '{"url":"u"}' });
  });

  it("keeps the server error contract and the retry hint", async () => {
    fetchMock(() =>
      Response.json({ kind: "rate_limited", code: "rate_limited", message: "calma" }, { status: 429, headers: { "retry-after": "7" } }),
    );
    const error = await getJson("/api/search", { query: "x" }).catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(HttpError);
    expect(error).toMatchObject({ kind: "rate_limited", code: "rate_limited", message: "calma", status: 429, retryAfterS: 7 });
  });

  it("maps platform pages and network failures to stable codes", async () => {
    fetchMock(() => new Response("<html>504</html>", { status: 504 }));
    await expect(getJson("/api/config", {})).rejects.toMatchObject({ kind: "upstream", code: "api", status: 504 });
    vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new TypeError("Failed to fetch"))));
    await expect(getJson("/api/config", {})).rejects.toMatchObject({ kind: "network", code: "network" });
    vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new DOMException("x", "AbortError"))));
    await expect(getJson("/api/config", {})).rejects.toSatisfy(isAbort);
  });
});

describe("retry", () => {
  it("follows the core equal-jitter backoff", () => {
    expect(backoffDelay(1, TRACK_POLICY, () => 0)).toBe(500);
    expect(backoffDelay(1, TRACK_POLICY, () => 1)).toBe(1000);
    expect(backoffDelay(3, TRACK_POLICY, () => 1)).toBe(4000);
    expect(backoffDelay(10, TRACK_POLICY, () => 1)).toBe(30_000);
    expect(backoffDelay(0, SEGMENT_POLICY, () => 0)).toBe(250);
  });

  it("sleeps until the delay or the cancellation", async () => {
    vi.useFakeTimers();
    const controller = new AbortController();
    const done = sleep(1000, controller.signal);
    await vi.advanceTimersByTimeAsync(1000);
    await expect(done).resolves.toBeUndefined();
    const cancelled = sleep(1000, controller.signal);
    controller.abort();
    await expect(cancelled).rejects.toSatisfy(isAbort);
    await expect(sleep(10, controller.signal)).rejects.toSatisfy(isAbort);
  });
});

describe("audio transfer", () => {
  const signal = new AbortController().signal;

  it("only talks to SoundCloud media hosts over https", () => {
    for (const url of [`${CDN}/a.mp3`, "https://playback.media-streaming.soundcloud.cloud/x.m4s", "https://i1.sndcdn.com/a.jpg"]) {
      expect(isTrustedMediaUrl(url)).toBe(true);
    }
    for (const url of ["http://cf-media.sndcdn.com/a.mp3", "https://sndcdn.com.evil.io/a", "https://user@cf-media.sndcdn.com/a", "nada"]) {
      expect(isTrustedMediaUrl(url)).toBe(false);
    }
  });

  it("streams a progressive file with progress", async () => {
    fetchMock(() => new Response(new Uint8Array(1000), { headers: { "content-length": "1000" } }));
    const progress: (number | null)[] = [];
    const [body] = await fetchProgressive(`${CDN}/a.mp3`, signal, (_bytes, fraction) => progress.push(fraction));
    expect(body?.length).toBe(1000);
    expect(progress.at(-1)).toBe(1);
  });

  it("rejects truncated, failed and foreign downloads", async () => {
    fetchMock(() => new Response(new Uint8Array(10), { headers: { "content-length": "20" } }));
    await expect(fetchProgressive(`${CDN}/a.mp3`, signal, () => undefined)).rejects.toThrow("download incompleto");
    fetchMock(() => new Response("x", { status: 403 }));
    await expect(fetchProgressive(`${CDN}/a.mp3`, signal, () => undefined)).rejects.toThrow("HTTP 403");
    await expect(fetchProgressive("https://evil.example/a.mp3", signal, () => undefined)).rejects.toBeInstanceOf(TransferError);
    vi.stubGlobal("fetch", vi.fn(() => Promise.reject(new TypeError("offline"))));
    await expect(fetchProgressive(`${CDN}/a.mp3`, signal, () => undefined)).rejects.toThrow("falha de rede");
    fetchMock(() => new Response(null, { headers: { "content-length": String(2 * 1024 ** 3) } }));
    await expect(fetchProgressive(`${CDN}/a.mp3`, signal, () => undefined)).rejects.toThrow("limite");
  });

  it("downloads HLS parts in parallel but returns them in order, retrying each part", async () => {
    let failures = 1;
    fetchMock((url) => {
      if (url.endsWith("/1.m4s") && failures > 0) {
        failures -= 1;
        return new Response("x", { status: 503 });
      }
      const index = Number(/\/(\d+)\.m4s$/.exec(url)?.[1] ?? 0);
      return new Response(new Uint8Array(index + 1).fill(index));
    });
    const urls = Array.from({ length: 8 }, (_, index) => `${CDN}/hls/${String(index)}.m4s`);
    const fractions: (number | null)[] = [];
    const parts = await fetchHls(urls, signal, (_bytes, fraction) => fractions.push(fraction));
    expect(parts.map((part) => part.length)).toEqual([1, 2, 3, 4, 5, 6, 7, 8]);
    expect(fractions.at(-1)).toBe(1);
  });

  it("fails the attempt when a part keeps failing, and on empty manifests", async () => {
    fetchMock(() => new Response("x", { status: 500 }));
    await expect(fetchHls([`${CDN}/0.m4s`], signal, () => undefined)).rejects.toThrow("HTTP 500");
    await expect(fetchHls([], signal, () => undefined)).rejects.toThrow("vazio");
  });

  it("uses the fallback cover and never fails the track over artwork", async () => {
    fetchMock((url) => (url.includes("original") ? new Response("x", { status: 404 }) : new Response(Uint8Array.of(1, 2, 3))));
    await expect(fetchArtwork([null, `${CDN}/original.jpg`, `${CDN}/t500.jpg`], signal)).resolves.toEqual(Uint8Array.of(1, 2, 3));
    fetchMock(() => new Response("x", { status: 404 }));
    await expect(fetchArtwork([`${CDN}/a.jpg`], signal)).resolves.toBeNull();
  });
});

describe("sinks", () => {
  it("names archives safely", () => {
    expect(safeFileName('a/b:c*?"<>|\u0001.')).toBe("a_b_c_______");
    expect(safeFileName(" ... ")).toBe("Perseus");
  });

  it("delivers one file as-is and several as a sorted zip", async () => {
    const single = new DownloadSink();
    expect(await single.existing()).toBeNull();
    await expect(single.finish("x")).resolves.toBeNull();
    await single.write("Pasta", "01. A.mp3", new Blob(["a"]));
    await expect(single.finish("x")).resolves.toMatchObject({ name: "01. A.mp3" });

    const playlist = new DownloadSink();
    await playlist.write("Perseus - Mix", "02. B.mp3", new Blob(["b"]));
    await playlist.write("Perseus - Mix", "01. A.mp3", new Blob(["a"]));
    const zip = await playlist.finish("Mix");
    expect(zip?.name).toBe("Perseus - Mix.zip");
    const text = await zipText(zip?.blob);
    expect(text.indexOf("01. A.mp3")).toBeLessThan(text.indexOf("02. B.mp3"));

    const collection = new DownloadSink();
    await collection.write("Band - Um", "01. A.mp3", new Blob(["a"]));
    await collection.write("Band - Dois", "01. B.mp3", new Blob(["b"]));
    const archive = await collection.finish("Albums: Band");
    expect(archive?.name).toBe("Albums_ Band.zip");
    expect(await zipText(archive?.blob)).toContain("Band - Um/01. A.mp3");
  });

  it("writes into the chosen folder atomically and reports existing files", async () => {
    const files = new Map<string, Blob>();
    const writable = (name: string, fail: boolean): Record<"write" | "close" | "abort", ReturnType<typeof vi.fn>> => ({
      write: vi.fn((data: Blob) => (fail ? Promise.reject(new Error("disco cheio")) : Promise.resolve(void files.set(name, data)))),
      close: vi.fn(() => Promise.resolve()),
      abort: vi.fn(() => Promise.resolve()),
    });
    const folder = {
      getFileHandle: vi.fn((name: string, options?: { create?: boolean }) => {
        if (!options?.create && !files.has(name)) return Promise.reject(new DOMException("x", "NotFoundError"));
        return Promise.resolve({
          getFile: () => Promise.resolve(files.get(name) ?? new Blob([])),
          createWritable: () => Promise.resolve(writable(name, name.startsWith("falha"))),
        });
      }),
    };
    const root = { getDirectoryHandle: vi.fn(() => Promise.resolve(folder)) };
    const sink = new DirectorySink(root as unknown as FileSystemDirectoryHandle);
    await expect(sink.existing("Mix", "01. A.mp3")).resolves.toBeNull();
    await sink.write("Mix", "01. A.mp3", new Blob(["abc"]));
    await expect(sink.existing("Mix", "01. A.mp3")).resolves.toBe(3);
    await expect(sink.write("Mix", "falha.mp3", new Blob(["x"]))).rejects.toThrow("disco cheio");
    await expect(sink.finish()).resolves.toBeNull();
    expect(root.getDirectoryHandle).toHaveBeenCalledTimes(1);
  });

  it("hands the artifact to the browser downloads", () => {
    vi.useFakeTimers();
    const create = vi.fn(() => "blob:perseus");
    const revoke = vi.fn();
    vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: create, revokeObjectURL: revoke }));
    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);
    saveArtifact({ name: "Mix.zip", blob: new Blob(["z"]) });
    expect(click).toHaveBeenCalledOnce();
    expect(document.querySelector("a[download]")).toBeNull();
    vi.advanceTimersByTime(60_000);
    expect(revoke).toHaveBeenCalledWith("blob:perseus");
  });
});
