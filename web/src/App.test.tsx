import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "./App";
import { I18nProvider, LOCALE_KEY } from "./i18n/context";
import type * as ApiModule from "./lib/api";
import { ApiError, api } from "./lib/api";
import { makeJob, makeRelease } from "./test/fixtures";
import type { AppConfig, JobEvent, JobEventKind } from "./types";

vi.mock("./lib/api", async (importOriginal) => {
  const original = await importOriginal<typeof ApiModule>();
  return {
    ...original,
    api: {
      config: vi.fn(),
      inspect: vi.fn(),
      jobs: vi.fn(),
      jobEvents: vi.fn(),
      createJob: vi.fn(),
      cancelJob: vi.fn(),
      openFolder: vi.fn(),
      pickOutputDir: vi.fn(),
      search: vi.fn(),
    },
  };
});

type Listener = (event: { payload: JobEvent }) => void;
const bus = vi.hoisted(() => ({ listeners: [] as ((event: { payload: unknown }) => void)[] }));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((_channel: string, handler: (event: { payload: unknown }) => void) => {
    bus.listeners.push(handler);
    return Promise.resolve(() => {
      bus.listeners = bus.listeners.filter((item) => item !== handler);
    });
  }),
}));

let sequence = 0;
function emit(event: JobEventKind, data: unknown, jobId = "job1"): void {
  sequence += 1;
  for (const listener of bus.listeners as Listener[]) listener({ payload: { job_id: jobId, id: sequence, event, data } });
}

const config: AppConfig = {
  version: "0.3.0",
  default_output_dir: "C:/Music/Perseus",
  max_workers: 16,
  default_workers: 4,
  interval_min: 10,
  interval_max: 3600,
};

const mocked = vi.mocked(api);
const PLAYLIST = "https://soundcloud.com/perseus/sets/argonautas";

describe("App", () => {
  beforeEach(() => {
    bus.listeners = [];
    sequence = 0;
    localStorage.clear();
    mocked.config.mockResolvedValue(config);
    mocked.jobs.mockResolvedValue([]);
    mocked.jobEvents.mockResolvedValue([]);
    mocked.inspect.mockResolvedValue(makeRelease());
    mocked.createJob.mockResolvedValue(makeJob());
    mocked.cancelJob.mockResolvedValue(makeJob({ state: "cancelled" }));
    mocked.openFolder.mockResolvedValue(undefined);
    mocked.pickOutputDir.mockResolvedValue("D:/Musica");
  });

  it("shows the empty state and credit", async () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "Cole um link do SoundCloud" })).toBeInTheDocument();
    expect(await screen.findByText("versão 0.3.0")).toBeInTheDocument();
  });

  it("inspects, downloads and follows live events to completion", async () => {
    render(<App />);
    await waitFor(() => expect(bus.listeners).toHaveLength(1));
    await userEvent.type(screen.getByLabelText("Link do SoundCloud"), PLAYLIST);
    await userEvent.click(screen.getByRole("button", { name: /Ver faixas/ }));
    expect(await screen.findByRole("heading", { name: "Argonautas" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Baixar" }));
    expect(mocked.createJob).toHaveBeenCalledWith(expect.objectContaining({ mode: "playlist", workers: 4 }));
    expect(await screen.findByRole("button", { name: "Cancelar" })).toBeInTheDocument();

    act(() => {
      emit("track", { track_id: 1001, status: "downloading", reason: null, bytes: 5, progress: 0.5 });
    });
    expect(screen.getByText("Baixando 50%")).toBeInTheDocument();

    act(() => {
      emit("log", { ts: 1, level: "INFO", message: "Concluido: Faixa 1" });
      emit("track", { track_id: 1001, status: "done", reason: null, bytes: 10, progress: null });
      emit("track", { track_id: 1002, status: "failed", reason: "timeout", bytes: null, progress: null });
      emit("state", makeJob({ state: "partial", counts: { downloaded: 1, reused: 0, failed: 1, unavailable: 1, selected: 3 } }));
      emit("end", {});
    });

    expect(screen.getByText("Falhou: timeout")).toBeInTheDocument();
    expect(screen.getByText("1 falhou")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Baixar" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: /Abrir pasta/ }));
    expect(mocked.openFolder).toHaveBeenCalledWith("job1");
  });

  it("ignores duplicated events and stale replayed state", async () => {
    mocked.jobs.mockResolvedValue([makeJob({ id: "old", state: "running" })]);
    const stale: JobEvent = { job_id: "old", id: 1, event: "state", data: makeJob({ id: "old", state: "running" }) };
    let resolveReplay: (items: JobEvent[]) => void = () => undefined;
    mocked.jobEvents.mockReturnValue(new Promise((resolve) => (resolveReplay = resolve)));
    render(<App />);
    await userEvent.click(await screen.findByText("Argonautas"));

    act(() => {
      for (const listener of bus.listeners as Listener[]) {
        listener({ payload: { job_id: "old", id: 2, event: "state", data: makeJob({ id: "old", state: "completed" }) } });
      }
    });
    await act(async () => {
      resolveReplay([stale]);
      await Promise.resolve();
    });
    expect(screen.getByRole("navigation", { name: "Tarefas desta sessão" })).toHaveTextContent("Concluída");
  });

  it("cancels the selected running job", async () => {
    render(<App />);
    await userEvent.type(screen.getByLabelText("Link do SoundCloud"), PLAYLIST);
    await userEvent.click(screen.getByRole("button", { name: "Baixar" }));
    await userEvent.click(await screen.findByRole("button", { name: "Cancelar" }));
    expect(mocked.cancelJob).toHaveBeenCalledWith("job1");
  });

  it("shows backend errors next to the input", async () => {
    mocked.inspect.mockRejectedValue(new ApiError("Dominio nao suportado: evil.com.", "invalid_input"));
    render(<App />);
    await userEvent.type(screen.getByLabelText("Link do SoundCloud"), "https://evil.com/a/b");
    await userEvent.click(screen.getByRole("button", { name: /Ver faixas/ }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Dominio nao suportado");
    expect(mocked.createJob).not.toHaveBeenCalled();
  });

  it("reports job creation limits", async () => {
    mocked.createJob.mockRejectedValue(new ApiError("Limite de 3 tarefas simultaneas atingido.", "too_many_jobs"));
    render(<App />);
    await userEvent.type(screen.getByLabelText("Link do SoundCloud"), PLAYLIST);
    await userEvent.click(screen.getByRole("button", { name: "Baixar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Limite de 3 tarefas");
  });

  it("restores a job from history, replays its log and persists settings", async () => {
    mocked.jobs.mockResolvedValue([makeJob({ id: "old", state: "completed", mode: "watch" })]);
    mocked.jobEvents.mockResolvedValue([
      { job_id: "old", id: 1, event: "log", data: { ts: 1, level: "INFO", message: "Linha antiga" } },
    ]);
    render(<App />);
    await userEvent.click(await screen.findByText("Argonautas"));
    expect(screen.getByRole("heading", { name: "Argonautas" })).toBeInTheDocument();
    expect(mocked.jobEvents).toHaveBeenCalledWith("old", 0);
    expect(await screen.findAllByText("Linha antiga")).not.toHaveLength(0);

    await userEvent.click(screen.getByText("Ajustes"));
    await userEvent.type(screen.getByLabelText("Baixar só as primeiras"), "7");
    expect(JSON.parse(localStorage.getItem("perseus.options.v1") ?? "{}")).toMatchObject({ limit: 7 });

    await userEvent.click(screen.getByRole("button", { name: /Escolher/ }));
    await waitFor(() => expect(screen.getByLabelText("Pasta de destino")).toHaveValue("D:/Musica"));
  });

  it("switches the whole interface language and persists the choice", async () => {
    mocked.inspect.mockRejectedValue(new ApiError("Dominio nao suportado: evil.com.", "invalid_input", "invalid_url"));
    const { unmount } = render(
      <I18nProvider locale="pt-BR">
        <App />
      </I18nProvider>,
    );
    expect(screen.getByRole("heading", { name: "Cole um link do SoundCloud" })).toBeInTheDocument();

    await userEvent.selectOptions(screen.getByLabelText("Idioma"), "en");
    expect(screen.getByRole("heading", { name: "Paste a SoundCloud link" })).toBeInTheDocument();
    expect(document.documentElement).toHaveAttribute("lang", "en");
    expect(localStorage.getItem(LOCALE_KEY)).toBe("en");

    await userEvent.type(screen.getByLabelText("SoundCloud link"), "https://evil.com/a/b");
    await userEvent.click(screen.getByRole("button", { name: /Show tracks/ }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("That isn't a SoundCloud link.");
    expect(alert).toHaveAttribute("title", "Dominio nao suportado: evil.com.");

    await userEvent.selectOptions(screen.getByLabelText("Language"), "ar");
    expect(screen.getByRole("heading", { name: "الصق رابطًا من SoundCloud" })).toBeInTheDocument();
    expect(document.documentElement).toHaveAttribute("dir", "rtl");
    unmount();

    render(
      <I18nProvider>
        <App />
      </I18nProvider>,
    );
    expect(screen.getByLabelText("اللغة")).toHaveValue("ar");
  });

  it("asks where to save before downloading and remembers the choice", async () => {
    render(<App />);
    await userEvent.type(screen.getByLabelText("Link do SoundCloud"), PLAYLIST);
    await userEvent.click(screen.getByRole("button", { name: "Baixar" }));
    await waitFor(() => expect(mocked.createJob).toHaveBeenCalled());
    expect(mocked.pickOutputDir).toHaveBeenCalledWith("Onde salvar as músicas?", "C:/Music/Perseus");
    expect(mocked.createJob).toHaveBeenCalledWith(expect.objectContaining({ output_dir: "D:/Musica" }));
    expect(JSON.parse(localStorage.getItem("perseus.options.v1") ?? "{}")).toMatchObject({ outputDir: "D:/Musica" });
  });

  it("does not start when the folder dialog is dismissed", async () => {
    mocked.pickOutputDir.mockResolvedValue(null);
    render(<App />);
    await userEvent.type(screen.getByLabelText("Link do SoundCloud"), PLAYLIST);
    await userEvent.click(screen.getByRole("button", { name: "Baixar" }));
    await waitFor(() => expect(mocked.pickOutputDir).toHaveBeenCalled());
    expect(mocked.createJob).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Baixar" })).toBeInTheDocument();
  });

  it("skips the dialog when asking is turned off", async () => {
    localStorage.setItem("perseus.options.v1", JSON.stringify({ outputDir: "E:/Fixa", askFolder: false }));
    render(<App />);
    await userEvent.type(screen.getByLabelText("Link do SoundCloud"), PLAYLIST);
    await userEvent.click(screen.getByRole("button", { name: "Baixar" }));
    await waitFor(() => expect(mocked.createJob).toHaveBeenCalledWith(expect.objectContaining({ output_dir: "E:/Fixa" })));
    expect(mocked.pickOutputDir).not.toHaveBeenCalled();
  });

  it("labels likes collections", async () => {
    mocked.inspect.mockResolvedValue(makeRelease({ kind: "likes", title: "Likes" }));
    render(<App />);
    await userEvent.type(screen.getByLabelText("Link do SoundCloud"), "https://soundcloud.com/miu/likes");
    await userEvent.click(screen.getByRole("button", { name: /Ver faixas/ }));
    expect(await screen.findByRole("heading", { name: "Curtidas" })).toBeInTheDocument();
  });

  it("searches when the input is not a link and loads the picked result", async () => {
    mocked.search.mockResolvedValue([
      { kind: "album", title: "Argonautas", subtitle: "Perseus Ensemble", url: "https://soundcloud.com/perseus/sets/argonautas", artwork_url: null, duration_ms: null, track_count: 3 },
    ]);
    render(<App />);
    await userEvent.type(screen.getByLabelText("Link do SoundCloud"), "argonautas perseus");
    await userEvent.click(screen.getByRole("button", { name: /Buscar/ }));
    expect(mocked.search).toHaveBeenCalledWith("argonautas perseus");
    expect(await screen.findByRole("heading", { name: "Resultados para “argonautas perseus”" })).toBeInTheDocument();
    expect(mocked.inspect).not.toHaveBeenCalled();

    await userEvent.click(screen.getByRole("button", { name: /Argonautas/ }));
    expect(mocked.inspect).toHaveBeenCalledWith("https://soundcloud.com/perseus/sets/argonautas", "playlist");
    expect(await screen.findByRole("heading", { name: "Argonautas" })).toBeInTheDocument();
    expect(screen.getByLabelText("Link do SoundCloud")).toHaveValue("https://soundcloud.com/perseus/sets/argonautas");
  });

  it("sends the advanced options with the job", async () => {
    localStorage.setItem(
      "perseus.options.v1",
      JSON.stringify({ askFolder: false, quality: "best", nameTemplate: " {title} ", maxKbps: 512, syncRemoved: true }),
    );
    render(<App />);
    await userEvent.type(screen.getByLabelText("Link do SoundCloud"), PLAYLIST);
    await userEvent.click(screen.getByRole("button", { name: "Baixar" }));
    await waitFor(() =>
      expect(mocked.createJob).toHaveBeenCalledWith(
        expect.objectContaining({ quality: "best", name_template: "{title}", max_kbps: 512, sync_removed: true, use_library: true }),
      ),
    );
  });

  it("ignores legacy stored options", () => {
    localStorage.setItem("perseus.options.v1", JSON.stringify({ workers: 8, engine: "scrapy", limit: "x" }));
    render(<App />);
    expect(screen.getByText(/8 downloads por vez/)).toBeInTheDocument();
  });

  it("follows watch-mode status messages", async () => {
    mocked.createJob.mockResolvedValue(makeJob({ mode: "watch" }));
    render(<App />);
    await waitFor(() => expect(bus.listeners).toHaveLength(1));
    await userEvent.type(screen.getByLabelText("Link do SoundCloud"), PLAYLIST);
    await userEvent.click(screen.getByLabelText("Monitorar"));
    await userEvent.click(screen.getByRole("button", { name: "Monitorar" }));
    expect(await screen.findByText(/Sincronizando a playlist/)).toBeInTheDocument();
    act(() => {
      emit("watch", { event: "watch_idle", message: "Verificacao #1: nenhuma novidade" });
    });
    expect(screen.getByText("Verificacao #1: nenhuma novidade")).toBeInTheDocument();
  });
});
