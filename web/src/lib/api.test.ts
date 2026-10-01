/* eslint-disable @typescript-eslint/prefer-promise-reject-errors -- o IPC do Tauri rejeita com o valor serializado pelo backend, nao com Error */
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { afterEach, describe, expect, it } from "vitest";

import { ApiError, api } from "./api";

type Handler = (cmd: string, args?: unknown) => unknown;

function backend(handler: Handler): { calls: [string, unknown][] } {
  const calls: [string, unknown][] = [];
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    return handler(cmd, args);
  });
  return { calls };
}

describe("api client", () => {
  afterEach(() => {
    clearMocks();
  });

  it("invokes commands with named arguments", async () => {
    const { calls } = backend(() => ({ kind: "playlist" }));
    await api.inspect("https://soundcloud.com/a/sets/b", "playlist");
    await api.createJob({
      url: "u",
      mode: "track",
      output_dir: null,
      workers: 2,
      limit: 3,
      interval: 60,
      quality: "best",
      name_template: null,
      min_duration_s: null,
      max_duration_s: 900,
      max_kbps: null,
      write_playlist_file: true,
      original_artwork: false,
      sync_removed: false,
      use_library: true,
    });
    await api.search("forss");
    await api.cancelJob("a/b");
    await api.jobEvents("x", 7);
    expect(calls.map(([cmd]) => cmd)).toEqual(["inspect", "create_job", "search", "cancel_job", "job_events"]);
    expect(calls[0]?.[1]).toEqual({ url: "https://soundcloud.com/a/sets/b", mode: "playlist" });
    expect(calls[1]?.[1]).toMatchObject({ spec: { url: "u", mode: "track", workers: 2, limit: 3, quality: "best", max_duration_s: 900 } });
    expect(calls[2]?.[1]).toEqual({ query: "forss" });
    expect(calls[3]?.[1]).toEqual({ id: "a/b" });
    expect(calls[4]?.[1]).toEqual({ id: "x", after: 7 });
  });

  it("surfaces the backend error message and kind", async () => {
    backend(() => Promise.reject({ kind: "invalid_input", message: "Dominio nao suportado" }));
    await expect(api.jobs()).rejects.toMatchObject({ message: "Dominio nao suportado", kind: "invalid_input" });
  });

  it("wraps plain string errors from the IPC layer", async () => {
    backend(() => Promise.reject("command get_config not found"));
    const error: unknown = await api.config().catch((err: unknown) => err);
    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).kind).toBe("ipc");
  });

  it("falls back to a generic message", async () => {
    backend(() => Promise.reject(null));
    await expect(api.config()).rejects.toThrow("Sem comunicacao com o backend");
  });

  it("returns folder picker and open folder results", async () => {
    const { calls } = backend((cmd) => (cmd === "pick_output_dir" ? "C:/Music" : null));
    await expect(api.pickOutputDir("Onde salvar?", "D:/Last")).resolves.toBe("C:/Music");
    expect(calls.at(-1)).toEqual(["pick_output_dir", { title: "Onde salvar?", initial: "D:/Last" }]);
    await expect(api.openFolder("abc")).resolves.toBeNull();
  });
});
