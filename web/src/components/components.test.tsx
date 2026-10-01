import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { makeJob, makeRelease, makeTrack } from "../test/fixtures";
import type { JobOptions } from "../types";
import { ActivityLog } from "./ActivityLog";
import { CommandBar } from "./CommandBar";
import { ReleaseView } from "./ReleaseView";
import { Settings } from "./Settings";
import { Sidebar } from "./Sidebar";

const noop = (): void => undefined;

function renderCommandBar(overrides: Partial<Parameters<typeof CommandBar>[0]> = {}): ReturnType<typeof vi.fn>[] {
  const handlers = { onUrlChange: vi.fn(), onModeChange: vi.fn(), onInspect: vi.fn(), onStart: vi.fn(), onCancel: vi.fn() };
  render(<CommandBar url="" mode="playlist" isQuery={false} inspecting={false} running={false} error={null} {...handlers} {...overrides} />);
  return [handlers.onUrlChange, handlers.onModeChange, handlers.onInspect, handlers.onStart, handlers.onCancel];
}

describe("CommandBar", () => {
  it("disables actions without a URL", () => {
    renderCommandBar();
    expect(screen.getByRole("button", { name: "Baixar" })).toBeDisabled();
    expect(screen.getByRole("button", { name: /Ver faixas/ })).toBeDisabled();
  });

  it("starts on submit and switches label in watch mode", async () => {
    const [, , , onStart] = renderCommandBar({ url: "https://soundcloud.com/a/sets/b", mode: "watch" });
    const button = screen.getByRole("button", { name: "Monitorar" });
    await userEvent.click(button);
    expect(onStart).toHaveBeenCalledOnce();
  });

  it("offers cancel while running and announces errors", async () => {
    const [, , , , onCancel] = renderCommandBar({ url: "x", running: true, error: { text: "Link invalido", detail: "detalhe tecnico" } });
    await userEvent.click(screen.getByRole("button", { name: "Cancelar" }));
    expect(onCancel).toHaveBeenCalledOnce();
    expect(screen.getByRole("alert")).toHaveTextContent("Link invalido");
    expect(screen.getByLabelText("Link do SoundCloud")).toHaveAttribute("aria-invalid", "true");
  });

  it("emits URL, mode and inspect events", async () => {
    const [onUrlChange, onModeChange, onInspect] = renderCommandBar({ url: "https://soundcloud.com/a/b" });
    await userEvent.type(screen.getByLabelText("Link do SoundCloud"), "x");
    expect(onUrlChange).toHaveBeenCalled();
    await userEvent.click(screen.getByLabelText("Só esta faixa"));
    expect(onModeChange).toHaveBeenCalledWith("track");
    await userEvent.click(screen.getByRole("button", { name: /Ver faixas/ }));
    expect(onInspect).toHaveBeenCalled();
  });

  it("pastes from the clipboard", async () => {
    const [onUrlChange] = renderCommandBar();
    Object.defineProperty(navigator, "clipboard", {
      value: { readText: vi.fn().mockResolvedValue("  https://soundcloud.com/a/b  ") },
      configurable: true,
    });
    await userEvent.click(screen.getByRole("button", { name: "Colar" }));
    expect(onUrlChange).toHaveBeenCalledWith("https://soundcloud.com/a/b");
  });
});

describe("ReleaseView", () => {
  it("shows release facts and per-track status", () => {
    const release = makeRelease();
    render(
      <ReleaseView
        release={release}
        progress={{
          1001: { status: "done" },
          1002: { status: "failed", reason: "timeout" },
          1003: { status: "unavailable", reason: "protegida por DRM" },
        }}
        job={makeJob({ counts: { downloaded: 1, reused: 0, failed: 1, unavailable: 1, selected: 3 } })}
        onOpenFolder={noop}
      />,
    );
    expect(screen.getByRole("heading", { name: "Argonautas" })).toBeInTheDocument();
    expect(screen.getByText("2 de 3")).toBeInTheDocument();
    expect(screen.getByText("3 de 3 faixas")).toBeInTheDocument();
    const rows = screen.getAllByRole("listitem");
    expect(within(rows[0]!).getByText("Baixada")).toBeInTheDocument();
    expect(within(rows[1]!).getByText("Falhou: timeout")).toBeInTheDocument();
    expect(within(rows[2]!).getByText("Indisponível: protegida por DRM")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "3");
  });

  it("shows live download percentage", () => {
    render(
      <ReleaseView
        release={makeRelease()}
        progress={{ 1001: { status: "downloading", progress: 0.42 }, 1002: { status: "downloading", progress: null } }}
        job={makeJob()}
        onOpenFolder={noop}
      />,
    );
    expect(screen.getByText("Baixando 42%")).toBeInTheDocument();
    expect(screen.getByText("Baixando")).toBeInTheDocument();
  });

  it("groups collection tracks by album and translates virtual titles", () => {
    const tracks = [
      makeTrack(1, { group: "Ecclesia" }),
      makeTrack(2, { group: "Ecclesia" }),
      makeTrack(3, { group: "Soulhack", id: 3003 }),
    ];
    render(
      <ReleaseView
        release={makeRelease({ kind: "albums", title: "Albums", tracks, total_tracks: 3 })}
        progress={{ 1001: { status: "copied" } }}
        job={null}
        onOpenFolder={noop}
      />,
    );
    expect(screen.getByRole("heading", { name: "Álbuns" })).toBeInTheDocument();
    expect(screen.getAllByText("Ecclesia")).toHaveLength(1);
    expect(screen.getByText("Soulhack")).toBeInTheDocument();
    expect(screen.getByText("Copiada da biblioteca")).toBeInTheDocument();
  });

  it("renders untrusted titles as text, never as HTML (XSS)", () => {
    const payload = "<img src=x onerror=\"window.__xss=1\"><script>window.__xss=2</script>";
    const { container } = render(
      <ReleaseView
        release={makeRelease({ title: payload, artist: payload, tracks: [makeTrack(1, { title: payload })] })}
        progress={{}}
        job={null}
        onOpenFolder={noop}
      />,
    );
    expect(screen.getByRole("heading", { name: payload })).toBeInTheDocument();
    expect(container.querySelector("img[src='x'], script")).toBeNull();
    expect((window as unknown as { __xss?: number }).__xss).toBeUndefined();
  });

  it("opens the folder when available", async () => {
    const onOpenFolder = vi.fn();
    render(<ReleaseView release={makeRelease()} progress={{}} job={makeJob()} onOpenFolder={onOpenFolder} />);
    await userEvent.click(screen.getByRole("button", { name: /Abrir pasta/ }));
    expect(onOpenFolder).toHaveBeenCalledOnce();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  });

  it("mentions tracks beyond the listed ones", () => {
    render(<ReleaseView release={makeRelease({ total_tracks: 10 })} progress={{}} job={null} onOpenFolder={noop} />);
    expect(screen.getByText(/Mais 7 faixas serão baixadas/)).toBeInTheDocument();
  });
});

describe("Sidebar", () => {
  it("lists jobs with summaries and author credit", async () => {
    const onSelect = vi.fn();
    render(
      <Sidebar
        version="0.2.0"
        jobs={[
          makeJob({ id: "a", counts: { downloaded: 1, reused: 1, failed: 0, unavailable: 0, selected: 5 } }),
          makeJob({ id: "b", state: "partial", title: null, url: "https://soundcloud.com/x/sets/y" }),
          makeJob({ id: "c", state: "failed" }),
          makeJob({ id: "d", state: "cancelled" }),
          makeJob({ id: "e", state: "completed" }),
          makeJob({ id: "f", mode: "watch" }),
        ]}
        selectedJobId="a"
        onSelect={onSelect}
      />,
    );
    expect(screen.getByText("2 de 5 faixas")).toBeInTheDocument();
    expect(screen.getByText("x/sets/y")).toBeInTheDocument();
    expect(screen.getByText("Monitorando novas faixas")).toBeInTheDocument();
    expect(screen.getByText("Desenvolvido por Isllan Toso")).toBeInTheDocument();
    await userEvent.click(screen.getByText("x/sets/y"));
    expect(onSelect).toHaveBeenCalledWith("b");
  });

  it("shows an empty state", () => {
    render(<Sidebar version={undefined} jobs={[]} selectedJobId={null} onSelect={noop} />);
    expect(screen.getByText("Os downloads que você iniciar aparecem aqui.")).toBeInTheDocument();
  });
});

describe("Settings", () => {
  const options: JobOptions = {
    outputDir: "",
    workers: 4,
    limit: null,
    interval: 60,
    askFolder: true,
    quality: "compatible",
    nameTemplate: "",
    minDurationS: null,
    maxDurationS: null,
    maxKbps: null,
    writePlaylistFile: true,
    originalArtwork: false,
    syncRemoved: false,
    useLibrary: true,
  };

  it("edits options and shows interval only in watch mode", async () => {
    const onChange = vi.fn();
    const onPickFolder = vi.fn();
    const { rerender } = render(
      <Settings config={null} options={options} mode="playlist" onChange={onChange} onPickFolder={onPickFolder} />,
    );
    expect(screen.queryByLabelText(/Verificar a cada/)).not.toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Baixar só as primeiras"), "5");
    expect(onChange).toHaveBeenLastCalledWith({ ...options, limit: 5 });
    await userEvent.click(screen.getByRole("button", { name: /Escolher/ }));
    expect(onPickFolder).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByLabelText("Perguntar onde salvar antes de cada download"));
    expect(onChange).toHaveBeenLastCalledWith({ ...options, askFolder: false });
    await userEvent.selectOptions(screen.getByLabelText("Qualidade"), "best");
    expect(onChange).toHaveBeenLastCalledWith({ ...options, quality: "best" });
    await userEvent.type(screen.getByLabelText("Duração máxima (s)"), "9");
    expect(onChange).toHaveBeenLastCalledWith({ ...options, maxDurationS: 9 });
    await userEvent.click(screen.getByLabelText("Reaproveitar faixas já baixadas em outras pastas"));
    expect(onChange).toHaveBeenLastCalledWith({ ...options, useLibrary: false });
    await userEvent.type(screen.getByLabelText("Nome dos arquivos"), "x");
    expect(onChange).toHaveBeenLastCalledWith({ ...options, nameTemplate: "x" });

    rerender(<Settings config={null} options={options} mode="watch" onChange={onChange} onPickFolder={onPickFolder} />);
    expect(screen.getByLabelText(/Verificar a cada/)).toBeInTheDocument();
  });
});

describe("ActivityLog", () => {
  it("renders lines and the latest message", () => {
    render(
      <ActivityLog
        lines={[
          { id: 1, ts: 0, level: "INFO", message: "Iniciando" },
          { id: 2, ts: 1, level: "ERROR", message: "Falhou feio" },
        ]}
      />,
    );
    expect(screen.getAllByText("Falhou feio")).toHaveLength(2);
  });

  it("renders the empty state", () => {
    render(<ActivityLog lines={[]} />);
    expect(screen.getByText("Nada registrado ainda.")).toBeInTheDocument();
  });
});

describe("Language picker", () => {
  it("shows language codes and keeps native names for assistive technology", () => {
    render(<Sidebar version={undefined} jobs={[]} selectedJobId={null} onSelect={noop} />);
    const picker = screen.getByLabelText("Idioma");
    const options = within(picker).getAllByRole("option");
    expect(options.map((option) => option.textContent)).toEqual([
      "EN", "ES", "ZH-CN", "HI", "FR", "PT-BR", "PT-PT", "AR", "BN", "RU", "ID",
    ]);
    expect(within(picker).getByRole("option", { name: "العربية" })).toHaveValue("ar");
    expect(picker).toHaveValue("pt-BR");
  });
});
