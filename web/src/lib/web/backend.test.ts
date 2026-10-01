import { afterEach, describe, expect, it, vi } from "vitest";

import type { CreateJobBody } from "../api";
import type { Job, JobEvent } from "../../types";
import { WebBackend } from "./backend";

const href = (input: RequestInfo | URL): string => (input instanceof Request ? input.url : input.toString());
const CDN = "https://cf-media.sndcdn.com";
const NO_PACING = { minGapMs: 0, maxGapMs: 0 };
const PLAYLIST = "https://soundcloud.com/perseus/sets/mix";

function mp3(): Uint8Array<ArrayBuffer> {
  const out = new Uint8Array(417 * 6);
  for (let index = 0; index < 6; index += 1) out.set([0xff, 0xfb, 0x90, 0x00], index * 417);
  return out;
}

const PLAN = {
  url: PLAYLIST,
  title: "Mix",
  folders: [
    {
      folder: "Perseus - Mix",
      album: "Mix",
      owner: "Perseus",
      single: false,
      total_tracks: 2,
      playlist_file: null,
      items: [
        {
          status: "ready",
          track_id: 1,
          file_name: "01. Perseus - Um.mp3",
          title: "Um",
          artist: "Perseus",
          album: "Mix",
          track_number: 1,
          total_tracks: 2,
          duration_ms: 1000,
          permalink_url: null,
          artwork_url: null,
          artwork_fallback_url: null,
          transcoding_url: "https://api-v2.soundcloud.com/media/soundcloud:tracks:1/a/stream/progressive",
          track_authorization: "eyJ.x.y",
          protocol: "progressive",
          container: "mp3",
          estimated_kbps: 128,
          tags: { artist: null, genre: null, isrc: null, label: null, composer: null, copyright: null, album_artist: null, year: null },
        },
        { status: "failed", track_id: 2, reason: "falha ao consultar lote", reason_code: null },
      ],
    },
  ],
};

const SPEC: CreateJobBody = {
  url: PLAYLIST,
  mode: "playlist",
  output_dir: null,
  workers: 40,
  limit: null,
  interval: 60,
  quality: "compatible",
  name_template: null,
  min_duration_s: null,
  max_duration_s: null,
  max_kbps: null,
  write_playlist_file: true,
  original_artwork: false,
  sync_removed: false,
  use_library: true,
};

function server(plan: unknown = PLAN, hold?: Promise<void>, onCall?: (url: string) => void): ReturnType<typeof vi.fn> {
  const mock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = href(input);
    onCall?.(url);
    if (url.startsWith("/api/config")) return Response.json({ version: "0.3.0", max_workers: 6 });
    if (url.startsWith("/api/inspect")) return Response.json({ kind: "playlist", url: new URL(url, "http://x").searchParams.get("url") });
    if (url.startsWith("/api/search")) return Response.json([{ kind: "track", title: new URL(url, "http://x").searchParams.get("query") }]);
    if (url === "/api/plan") {
      await hold;
      return Response.json(plan);
    }
    if (url === "/api/stream") {
      expect(JSON.parse(typeof init?.body === "string" ? init.body : "null")).toEqual({
        transcoding_url: "https://api-v2.soundcloud.com/media/soundcloud:tracks:1/a/stream/progressive",
        track_authorization: "eyJ.x.y",
      });
      return Response.json({ protocol: "progressive", url: `${CDN}/1.mp3` });
    }
    if (url.startsWith(CDN)) return new Response(mp3());
    return new Response("?", { status: 404 });
  });
  vi.stubGlobal("fetch", mock);
  return mock;
}

async function finished(backend: WebBackend, id: string): Promise<Job> {
  await vi.waitFor(async () => {
    const jobs = (await backend.invoke("list_jobs")) as Job[];
    expect(jobs.find((job) => job.id === id)?.state).not.toBe("running");
  });
  const job = ((await backend.invoke("list_jobs")) as Job[]).find((item) => item.id === id);
  if (!job) throw new Error(`tarefa ${id} sumiu`);
  return job;
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("WebBackend", () => {
  it("serves metadata commands from /api", async () => {
    const mock = server();
    const backend = new WebBackend({ autoSave: false, pacing: NO_PACING });
    await expect(backend.invoke("get_config")).resolves.toMatchObject({ max_workers: 6 });
    await expect(backend.invoke("inspect", { url: PLAYLIST, mode: "track" })).resolves.toMatchObject({ url: PLAYLIST });
    await expect(backend.invoke("search", { query: "argo" })).resolves.toEqual([{ kind: "track", title: "argo" }]);
    expect(String(mock.mock.calls[1]?.[0])).toContain("mode=track");
    await expect(backend.invoke("nope")).rejects.toMatchObject({ kind: "internal" });
    await expect(backend.invoke("cancel_job", { id: "x" })).rejects.toMatchObject({ code: "job_not_found" });
  });

  it("runs a job with the same events as the desktop job manager", async () => {
    server();
    const backend = new WebBackend({ autoSave: false, pacing: NO_PACING });
    const events: JobEvent[] = [];
    const stop = backend.listen((event) => events.push(event));
    const created = (await backend.invoke("create_job", { spec: SPEC })) as Job;
    expect(created).toMatchObject({ state: "running", mode: "playlist", url: PLAYLIST });

    const job = await finished(backend, created.id);
    expect(job).toMatchObject({
      state: "partial",
      title: "Mix",
      target_dir: "01. Perseus - Um.mp3",
      counts: { downloaded: 1, failed: 1, reused: 0, unavailable: 0, selected: 2 },
    });
    expect(events.map((event) => event.id)).toEqual(events.map((_, index) => index + 1));
    expect(events.at(-1)?.event).toBe("end");
    const tracks = events.filter((event) => event.event === "track").map((event) => (event.data as { status: string }).status);
    expect(tracks).toContain("failed");
    expect(tracks).toContain("done");
    const logs = events.filter((event) => event.event === "log").map((event) => (event.data as { detail: { event: string } }).detail.event);
    expect(logs).toEqual(expect.arrayContaining(["planned", "track_failed", "track_downloaded", "archive_ready", "finished"]));

    const replay = (await backend.invoke("job_events", { id: created.id, after: events.length - 2 })) as JobEvent[];
    expect(replay.map((event) => event.event)).toEqual(["state", "end"]);

    const click = vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);
    vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: () => "blob:x", revokeObjectURL: () => undefined }));
    await backend.invoke("open_folder", { id: created.id });
    expect(click).toHaveBeenCalledOnce();
    stop();
  });

  it("marks planning errors as failed jobs with the server code", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(() => Promise.resolve(Response.json({ kind: "invalid_input", code: "too_many_tracks", message: "muitas" }, { status: 400 }))),
    );
    const backend = new WebBackend({ autoSave: false, pacing: NO_PACING });
    const created = (await backend.invoke("create_job", { spec: SPEC })) as Job;
    const job = await finished(backend, created.id);
    expect(job).toMatchObject({ state: "failed", error_code: "too_many_tracks", error: "muitas", target_dir: null });
    await expect(backend.invoke("open_folder", { id: created.id })).rejects.toMatchObject({ code: "download_not_ready" });
  });

  it("limits active jobs, refuses watching and cancels on request", async () => {
    let release = (): void => undefined;
    const hold = new Promise<void>((resolve) => {
      release = resolve;
    });
    server(PLAN, hold);
    const backend = new WebBackend({ autoSave: false, pacing: NO_PACING });
    await expect(backend.invoke("create_job", { spec: { ...SPEC, mode: "watch" } })).rejects.toMatchObject({ code: "watch_unsupported" });
    const jobs = await Promise.all([1, 2, 3].map(() => backend.invoke("create_job", { spec: SPEC }) as Promise<Job>));
    await expect(backend.invoke("create_job", { spec: SPEC })).rejects.toMatchObject({ code: "too_many_jobs" });
    const [first] = jobs;
    await backend.invoke("cancel_job", { id: first?.id });
    release();
    expect((await finished(backend, first?.id ?? "")).state).toBe("cancelled");
  });

  it("uses the picked folder only for the next job and treats a closed picker as cancel", async () => {
    server();
    const backend = new WebBackend({ autoSave: false, pacing: NO_PACING });
    await expect(backend.invoke("pick_output_dir", { title: "t", initial: null })).resolves.toBeNull();

    window.showDirectoryPicker = vi.fn(() => Promise.reject(new DOMException("closed", "AbortError")));
    await expect(backend.invoke("pick_output_dir")).resolves.toBeNull();
    window.showDirectoryPicker = vi.fn(() => Promise.reject(new DOMException("no gesture", "SecurityError")));
    await expect(backend.invoke("pick_output_dir")).rejects.toMatchObject({ code: "picker_needs_gesture" });

    const written: string[] = [];
    const folder = {
      getFileHandle: vi.fn((name: string, options?: { create?: boolean }) =>
        options?.create
          ? Promise.resolve({
              createWritable: () =>
                Promise.resolve({ write: () => Promise.resolve(void written.push(name)), close: () => Promise.resolve(), abort: () => Promise.resolve() }),
            })
          : Promise.reject(new DOMException("x", "NotFoundError")),
      ),
    };
    const root = { name: "Musicas", getDirectoryHandle: vi.fn(() => Promise.resolve(folder)) };
    window.showDirectoryPicker = vi.fn(() => Promise.resolve(root as unknown as FileSystemDirectoryHandle));
    await expect(backend.invoke("pick_output_dir")).resolves.toBe("Musicas");
    const created = (await backend.invoke("create_job", { spec: { ...SPEC, output_dir: "Musicas" } })) as Job;
    const job = await finished(backend, created.id);
    expect(written).toEqual(["01. Perseus - Um.mp3"]);
    expect(job.target_dir).toBeNull();
    delete window.showDirectoryPicker;
  });

  it("paces planning and stream calls with random gaps shared by the whole tab", async () => {
    const stamps: number[] = [];
    server(PLAN, undefined, (url) => {
      if (url === "/api/plan" || url === "/api/stream") stamps.push(performance.now());
    });
    const backend = new WebBackend({ autoSave: false, pacing: { minGapMs: 40, maxGapMs: 60 } });
    const jobs = await Promise.all([1, 2].map(() => backend.invoke("create_job", { spec: SPEC }) as Promise<Job>));
    for (const job of jobs) await finished(backend, job.id);
    expect(stamps).toHaveLength(4);
    const first = stamps[0] ?? 0;
    stamps.forEach((at, index) => expect(at - first).toBeGreaterThanOrEqual(index * 40 - 2));
  });

  it("warns before closing the tab while downloading", async () => {
    let release = (): void => undefined;
    server(PLAN, new Promise<void>((resolve) => {
      release = resolve;
    }));
    const backend = new WebBackend({ autoSave: false, pacing: NO_PACING });
    const created = (await backend.invoke("create_job", { spec: SPEC })) as Job;
    const event = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
    release();
    await finished(backend, created.id);
  });
});
