import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "../App";
import type * as ApiModule from "../lib/api";
import { api } from "../lib/api";
import { makeJob, makeRelease } from "../test/fixtures";
import type { JobOptions } from "../types";
import { CommandBar } from "./CommandBar";
import { ReleaseView } from "./ReleaseView";
import { Settings } from "./Settings";
import { Sidebar } from "./Sidebar";

const platform = vi.hoisted(() => ({ folder: false }));

vi.mock("../lib/platform", () => ({
  IS_WEB: true,
  DESKTOP_DOWNLOAD_URL: "https://github.com/Isllanrx/Perseus/releases/latest",
  canPickFolder: () => platform.folder,
}));

vi.mock("../lib/api", async (importOriginal) => {
  const original = await importOriginal<typeof ApiModule>();
  return {
    ...original,
    api: {
      config: vi.fn(() => Promise.resolve({ version: "0.3.0", default_output_dir: "", max_workers: 6, default_workers: 4, interval_min: 10, interval_max: 3600 })),
      inspect: vi.fn(),
      jobs: vi.fn(() => Promise.resolve([])),
      jobEvents: vi.fn(() => Promise.resolve([])),
      createJob: vi.fn(),
      cancelJob: vi.fn(),
      openFolder: vi.fn(),
      pickOutputDir: vi.fn(),
      search: vi.fn(),
    },
  };
});

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => undefined)) }));

const OPTIONS: JobOptions = {
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

const noop = (): void => undefined;

beforeEach(() => {
  platform.folder = false;
  localStorage.clear();
});

describe("web build", () => {
  it("hides watching, which needs an always-on process", () => {
    render(
      <CommandBar url="" mode="playlist" isQuery={false} inspecting={false} running={false} error={null}
        onUrlChange={noop} onModeChange={noop} onInspect={noop} onStart={noop} onCancel={noop} />,
    );
    expect(screen.getByRole("radio", { name: "Playlist inteira" })).toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: "Monitorar" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ver faixas" })).toBeInTheDocument();
  });

  it("explains browser downloads when folders are not available", () => {
    render(<Settings config={null} options={OPTIONS} mode="playlist" onChange={noop} onPickFolder={noop} />);
    expect(screen.getByText(/downloads do navegador; playlists e álbuns chegam num único .zip/)).toBeInTheDocument();
    for (const desktopOnly of ["Pasta de destino", "Limite de banda (kbit/s)", "Reaproveitar faixas já baixadas em outras pastas"]) {
      expect(screen.queryByText(desktopOnly)).not.toBeInTheDocument();
    }
    expect(screen.getByLabelText("Gerar playlist .m3u8")).toBeChecked();
  });

  it("lets Chromium save straight to a folder", async () => {
    platform.folder = true;
    const onChange = vi.fn();
    render(<Settings config={null} options={OPTIONS} mode="playlist" onChange={onChange} onPickFolder={noop} />);
    await userEvent.click(screen.getByLabelText("Salvar direto numa pasta do dispositivo (pergunta a cada download)"));
    expect(onChange).toHaveBeenCalledWith({ ...OPTIONS, askFolder: false });
  });

  it("offers the finished file instead of opening a folder", async () => {
    const onOpenFolder = vi.fn();
    render(
      <ReleaseView release={makeRelease()} progress={{}} job={makeJob({ state: "completed", target_dir: "Mix.zip" })} onOpenFolder={onOpenFolder} />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Salvar arquivo" }));
    expect(onOpenFolder).toHaveBeenCalledOnce();
    expect(screen.queryByText("Abrir pasta")).not.toBeInTheDocument();
  });

  it("links to the desktop app", () => {
    render(<Sidebar version="0.3.0" jobs={[]} selectedJobId={null} onSelect={noop} />);
    expect(screen.getByRole("link", { name: "Baixar o app para Windows" })).toHaveAttribute(
      "href",
      "https://github.com/Isllanrx/Perseus/releases/latest",
    );
  });

  it("starts without a folder picker where the browser has none", async () => {
    const mocked = vi.mocked(api);
    mocked.inspect.mockResolvedValue(makeRelease());
    mocked.createJob.mockResolvedValue(makeJob());
    render(<App />);
    await userEvent.type(screen.getByLabelText("Link do SoundCloud"), "https://soundcloud.com/perseus/sets/argonautas");
    await userEvent.click(screen.getByRole("button", { name: "Baixar" }));
    await waitFor(() => expect(mocked.createJob).toHaveBeenCalledOnce());
    expect(mocked.pickOutputDir).not.toHaveBeenCalled();
    expect(mocked.createJob.mock.calls[0]?.[0]).toMatchObject({ output_dir: null, mode: "playlist" });
  });
});
