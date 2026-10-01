import { afterEach, describe, expect, it, vi } from "vitest";

import { type EngineDeps, playlistText, runEngine } from "./engine";
import { type CoreEvent, levelOf, messageOf } from "./events";
import { HttpError } from "./http";
import type { Artifact, Sink } from "./sink";
import type { PlanFolder, PlanIn, PlanOut, ReadyItem, StreamOut } from "./types";

const href = (input: RequestInfo | URL): string => (input instanceof Request ? input.url : input.toString());
const CDN = "https://cf-media.sndcdn.com";

function mp3(frames = 8): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(417 * frames);
  for (let index = 0; index < frames; index += 1) out.set([0xff, 0xfb, 0x90, 0x00], index * 417);
  return out;
}

function ready(id: number, extra: Partial<ReadyItem> = {}): ReadyItem {
  return {
    status: "ready",
    track_id: id,
    file_name: `0${String(id)}. Perseus - Faixa ${String(id)}.mp3`,
    title: `Faixa ${String(id)}`,
    artist: "Perseus",
    album: "Mix",
    track_number: id,
    total_tracks: 3,
    duration_ms: 61_000,
    permalink_url: `https://soundcloud.com/perseus/${String(id)}`,
    artwork_url: `${CDN}/art.jpg`,
    artwork_fallback_url: null,
    transcoding_url: `https://api-v2.soundcloud.com/media/soundcloud:tracks:${String(id)}/a/stream/progressive`,
    track_authorization: null,
    protocol: "progressive",
    container: "mp3",
    estimated_kbps: 128,
    tags: { artist: null, genre: null, isrc: null, label: null, composer: null, copyright: null, album_artist: "Perseus", year: 2024 },
    ...extra,
  };
}

function folder(items: PlanFolder["items"], extra: Partial<PlanFolder> = {}): PlanFolder {
  return { folder: "Perseus - Mix", album: "Mix", owner: "Perseus", single: false, total_tracks: 3, playlist_file: "Mix.m3u8", items, ...extra };
}

class MemorySink implements Sink {
  readonly kind = "download";
  readonly written = new Map<string, Blob>();
  constructor(
    private readonly present = new Set<string>(),
    private readonly failOn = "",
  ) {}
  existing(_folder: string, file: string): Promise<number | null> {
    return Promise.resolve(this.present.has(file) ? 10 : null);
  }
  write(dir: string, file: string, data: Blob): Promise<void> {
    if (file === this.failOn) return Promise.reject(new Error("sem permissao"));
    this.written.set(`${dir}/${file}`, data);
    return Promise.resolve();
  }
  finish(): Promise<Artifact | null> {
    return Promise.resolve(this.written.size ? { name: "Mix.zip", blob: new Blob(["zip"]) } : null);
  }
}

const SPEC: PlanIn & { workers: number } = {
  url: "https://soundcloud.com/perseus/sets/mix",
  mode: "playlist",
  limit: null,
  quality: "compatible",
  name_template: null,
  min_duration_s: null,
  max_duration_s: null,
  write_playlist_file: true,
  original_artwork: false,
  workers: 2,
};

function deps(plan: PlanOut, overrides: Partial<EngineDeps> = {}): EngineDeps & { events: CoreEvent[] } {
  const events: CoreEvent[] = [];
  return {
    events,
    plan: () => Promise.resolve(plan),
    stream: (item): Promise<StreamOut> => Promise.resolve({ protocol: "progressive", url: `${CDN}/${String(item.track_id)}.mp3` }),
    sink: new MemorySink(),
    emit: (event) => events.push(event),
    signal: new AbortController().signal,
    random: () => 0,
    ...overrides,
  };
}

function cdn(handler?: (url: string) => Response | undefined): void {
  vi.stubGlobal(
    "fetch",
    vi.fn((input: RequestInfo | URL) => {
      const url = href(input);
      const custom = handler?.(url);
      if (custom) return Promise.resolve(custom);
      if (url.endsWith("art.jpg")) return Promise.resolve(new Response(Uint8Array.of(0xff, 0xd8, 0xff)));
      return Promise.resolve(new Response(mp3()));
    }),
  );
}

const names = (events: CoreEvent[]): string[] => events.map((event) => event.event);

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("runEngine", () => {
  it("downloads, tags and writes the playlist file in playlist order", async () => {
    cdn();
    const plan: PlanOut = {
      url: SPEC.url,
      title: "Mix",
      folders: [folder([ready(1), ready(2), { status: "unavailable", track_id: 3, reason: "drm", reason_code: "drm" }])],
    };
    const run = deps(plan);
    const report = await runEngine(SPEC, run);

    expect(report).toMatchObject({ downloaded: 2, unavailable: 1, failed: 0, cancelled: false, fatal: null, title: "Mix" });
    expect(report.artifact?.name).toBe("Mix.zip");
    expect(names(run.events)).toEqual(
      expect.arrayContaining(["planned", "track_unavailable", "track_downloaded", "playlist_file_written", "archive_ready", "finished"]),
    );
    expect(names(run.events).at(-1)).toBe("finished");
    const sink = run.sink as MemorySink;
    const m3u = await sink.written.get("Perseus - Mix/Mix.m3u8")?.text();
    expect(m3u).toBe(
      "#EXTM3U\n#PLAYLIST:Mix\n#EXTINF:61,Perseus - Faixa 1\n01. Perseus - Faixa 1.mp3\n#EXTINF:61,Perseus - Faixa 2\n02. Perseus - Faixa 2.mp3\n",
    );
    const audio = new Uint8Array(await (sink.written.get("Perseus - Mix/01. Perseus - Faixa 1.mp3") ?? new Blob()).arrayBuffer());
    expect(String.fromCharCode(...audio.subarray(0, 3))).toBe("ID3");
  });

  it("retries with a fresh stream and honors Retry-After", async () => {
    vi.useFakeTimers();
    cdn();
    let calls = 0;
    const run = deps(
      { url: SPEC.url, title: "Mix", folders: [folder([ready(1)])] },
      {
        stream: () => {
          calls += 1;
          if (calls === 1) return Promise.reject(new HttpError("rate_limited", "rate_limited", "calma", 429, 5));
          return Promise.resolve({ protocol: "hls", parts: [`${CDN}/0.mp3`, `${CDN}/1.mp3`] });
        },
      },
    );
    const done = runEngine(SPEC, run);
    await vi.advanceTimersByTimeAsync(5000);
    const report = await done;
    expect(report.downloaded).toBe(1);
    const retry = run.events.find((event) => event.event === "track_retry");
    expect(retry).toMatchObject({ attempt: 1, max: 3, reason: "calma", delay_s: 5 });
    expect(run.events.find((event) => event.event === "track_downloaded")).toMatchObject({ attempts: 2 });
  });

  it("gives up after three attempts and does not retry unavailable streams or write errors", async () => {
    vi.useFakeTimers();
    cdn((url) => (url.includes("/1.mp3") ? new Response("x", { status: 403 }) : undefined));
    const run = deps(
      { url: SPEC.url, title: "Mix", folders: [folder([ready(1), ready(2), ready(3)])] },
      {
        sink: new MemorySink(new Set(), "03. Perseus - Faixa 3.mp3"),
        stream: (item) =>
          item.track_id === 2
            ? Promise.reject(new HttpError("unavailable", "stream_unavailable", "sem stream", 422))
            : Promise.resolve({ protocol: "progressive", url: `${CDN}/${String(item.track_id)}.mp3` }),
      },
    );
    const done = runEngine({ ...SPEC, workers: 3 }, run);
    await vi.advanceTimersByTimeAsync(10_000);
    const report = await done;
    expect(report).toMatchObject({ downloaded: 0, failed: 2, unavailable: 1 });
    expect(run.events.filter((event) => event.event === "track_retry")).toHaveLength(2);
    expect(run.events.find((event) => event.event === "track_failed" && event.track_id === 3)).toMatchObject({
      reason: "nao foi possivel gravar: sem permissao",
    });
    expect(run.events.find((event) => event.event === "track_unavailable")).toMatchObject({ track_id: 2 });
    expect(names(run.events)).not.toContain("playlist_file_written");
  });

  it("reuses files already in the folder and rejects invalid audio", async () => {
    cdn((url) => (url.includes("/2.mp3") ? new Response(new Uint8Array(5000)) : undefined));
    vi.useFakeTimers();
    const run = deps(
      { url: SPEC.url, title: "Mix", folders: [folder([ready(1), ready(2)], { playlist_file: null })] },
      { sink: new MemorySink(new Set(["01. Perseus - Faixa 1.mp3"])) },
    );
    const done = runEngine(SPEC, run);
    await vi.advanceTimersByTimeAsync(10_000);
    const report = await done;
    expect(report).toMatchObject({ reused: 1, failed: 1 });
    expect(run.events.find((event) => event.event === "track_failed")).toMatchObject({ reason: "arquivo de audio invalido" });
  });

  it("reports planning failures as fatal and cancellation as cancelled", async () => {
    const failed = deps({ url: "", title: "", folders: [] }, {
      plan: () => Promise.reject(new HttpError("invalid_input", "too_many_tracks", "muitas", 400)),
    });
    const report = await runEngine(SPEC, failed);
    expect(report.fatal).toEqual({ code: "too_many_tracks", message: "muitas" });
    expect(names(failed.events)).toEqual(["resolve_failed"]);

    const controller = new AbortController();
    controller.abort();
    const cancelled = deps({ url: "", title: "", folders: [] }, {
      signal: controller.signal,
      plan: () => Promise.reject(new DOMException("x", "AbortError")),
    });
    expect((await runEngine(SPEC, cancelled)).cancelled).toBe(true);
    expect(names(cancelled.events)).toEqual(["cancelled"]);
  });

  it("stops scheduling new tracks after cancel", async () => {
    const controller = new AbortController();
    cdn();
    const run = deps(
      { url: SPEC.url, title: "Mix", folders: [folder([ready(1), ready(2), ready(3)])] },
      {
        signal: controller.signal,
        stream: () => {
          controller.abort();
          return Promise.reject(new DOMException("x", "AbortError"));
        },
      },
    );
    const report = await runEngine({ ...SPEC, workers: 1 }, run);
    expect(report).toMatchObject({ cancelled: true, downloaded: 0, failed: 0 });
    expect(names(run.events).at(-1)).toBe("cancelled");
  });
});

describe("playlistText", () => {
  it("skips missing tracks and unknown durations become -1", () => {
    const entries = new Map([[2, { file: "b.mp3", artist: "A", title: "B", durationMs: null }]]);
    expect(playlistText("Mix", folder([ready(1), ready(2)]), entries)).toBe("#EXTM3U\n#PLAYLIST:Mix\n#EXTINF:-1,A - B\nb.mp3\n");
    expect(playlistText("Mix", folder([ready(1)]), new Map())).toBeNull();
  });
});

describe("events", () => {
  it("mirror the core levels and pt-BR messages", () => {
    const samples: CoreEvent[] = [
      { event: "planned", album: "Mix", artist: "P", selected: 2, total: 3, target_dir: "P - Mix" },
      { event: "resolve_failed", url: "u", reason: "r" },
      { event: "track_progress", track_id: 1, bytes: 10, fraction: 0.5 },
      { event: "track_downloaded", track_id: 1, file: "a.mp3", bytes: 1, attempts: 1, elapsed_s: 1, protocol: "hls" },
      { event: "track_reused", track_id: 1, file: "a.mp3" },
      { event: "playlist_file_written", file: "Mix.m3u8", tracks: 2 },
      { event: "track_retry", track_id: 1, title: "A", attempt: 1, max: 3, reason: "r", delay_s: 1.25 },
      { event: "track_unavailable", track_id: 1, reason: "drm", reason_code: "drm" },
      { event: "track_failed", track_id: 1, reason: "rede", reason_code: null },
      { event: "finished", downloaded: 1, reused: 0, failed: 0, unavailable: 0, bytes: 1, elapsed_s: 2 },
      { event: "cancelled" },
      { event: "archive_ready", file: "Mix.zip", bytes: 3 },
    ];
    expect(samples.map(levelOf)).toEqual([
      "INFO", "ERROR", "DEBUG", "INFO", "INFO", "INFO", "WARNING", "WARNING", "WARNING", "INFO", "WARNING", "INFO",
    ]);
    for (const sample of samples) expect(messageOf(sample)).not.toBe("");
    const retry = samples.find((sample) => sample.event === "track_retry");
    expect(retry && messageOf(retry)).toContain("Nova tentativa em 1.3s");
  });
});
