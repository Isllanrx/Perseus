import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

const PLAYLIST = "https://soundcloud.com/perseus/sets/argonautas";
const CDN = "https://cf-media.sndcdn.com";
const DRM = "protegida por DRM (somente streams criptografados)";
const FOLDER = "Perseus Ensemble - Argonautas";

function mp3(frames = 120): Buffer {
  const frame = Buffer.alloc(417);
  frame.set([0xff, 0xfb, 0x90, 0x00]);
  return Buffer.concat(Array.from({ length: frames }, () => frame));
}

const release = {
  kind: "playlist",
  url: PLAYLIST,
  playlist_context: null,
  title: "Argonautas",
  artist: "Perseus Ensemble",
  artwork_url: null,
  permalink_url: PLAYLIST,
  total_tracks: 3,
  tracks: [1, 2, 3].map((id) => ({
    id,
    position: id,
    title: `Faixa ${String(id)}`,
    artist: "Perseus Ensemble",
    duration_ms: 120_000,
    artwork_url: null,
    permalink_url: null,
    available: id !== 3,
    reason: id === 3 ? DRM : null,
    reason_code: id === 3 ? "drm" : null,
    group: null,
  })),
};

function ready(id: number): Record<string, unknown> {
  return {
    status: "ready",
    track_id: id,
    file_name: `0${String(id)}. Perseus Ensemble - Faixa ${String(id)}.mp3`,
    title: `Faixa ${String(id)}`,
    artist: "Perseus Ensemble",
    album: "Argonautas",
    track_number: id,
    total_tracks: 3,
    duration_ms: 120_000,
    permalink_url: null,
    artwork_url: null,
    artwork_fallback_url: null,
    transcoding_url: `https://api-v2.soundcloud.com/media/soundcloud:tracks:${String(id)}/a1/stream/progressive`,
    track_authorization: null,
    protocol: "progressive",
    container: "mp3",
    estimated_kbps: 128,
    tags: {
      artist: null,
      genre: null,
      isrc: null,
      label: null,
      composer: null,
      copyright: null,
      album_artist: "Perseus Ensemble",
      year: 2024,
    },
  };
}

const plan = {
  url: PLAYLIST,
  title: "Argonautas",
  folders: [
    {
      folder: FOLDER,
      album: "Argonautas",
      owner: "Perseus Ensemble",
      single: false,
      total_tracks: 3,
      playlist_file: "Argonautas.m3u8",
      items: [ready(1), ready(2), { status: "unavailable", track_id: 3, reason: DRM, reason_code: "drm" }],
    },
  ],
};

async function mockServer(page: Page, planStatus = 200): Promise<void> {
  await page.route("**/api/config", (route) =>
    route.fulfill({
      json: { version: "0.3.0", default_output_dir: "", max_workers: 6, default_workers: 4, interval_min: 10, interval_max: 3600 },
    }),
  );
  await page.route("**/api/inspect?**", (route) => route.fulfill({ json: release }));
  await page.route("**/api/plan", (route) =>
    planStatus === 200
      ? route.fulfill({ json: plan })
      : route.fulfill({
          status: planStatus,
          json: { kind: "invalid_input", code: "too_many_tracks", message: "A versao web baixa ate 1000 faixas por vez." },
        }),
  );
  await page.route("**/api/stream", async (route) => {
    const body = route.request().postDataJSON() as { transcoding_url: string };
    const id = /tracks:(\d+)/.exec(body.transcoding_url)?.[1] ?? "0";
    await route.fulfill({ json: { protocol: "progressive", url: `${CDN}/${id}.128.mp3?Policy=x` } });
  });
  await page.route(`${CDN}/**`, (route) =>
    route.fulfill({ body: mp3(), headers: { "access-control-allow-origin": "*", "content-type": "audio/mpeg" } }),
  );
}

async function browserDownloads(page: Page): Promise<void> {
  await page.addInitScript(() => {
    localStorage.setItem("perseus.options.v1", JSON.stringify({ askFolder: false, workers: 4 }));
  });
}

async function inspect(page: Page): Promise<void> {
  await page.getByLabel("Link do SoundCloud").fill(PLAYLIST);
  await page.getByRole("button", { name: "Ver faixas" }).click();
  await expect(page.getByRole("heading", { name: "Argonautas" })).toBeVisible();
}

test.beforeEach(async ({ page }) => {
  await mockServer(page);
});

test("offers only what works in a browser", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "Cole um link do SoundCloud" })).toBeVisible();
  await expect(page.getByRole("radio", { name: "Monitorar" })).toHaveCount(0);
  await page.getByText("Ajustes").click();
  await expect(page.getByText("Pasta de destino")).toHaveCount(0);
  await expect(page.getByText("Limite de banda (kbit/s)")).toHaveCount(0);
  await expect(page.getByText("Reaproveitar faixas já baixadas em outras pastas")).toHaveCount(0);
  const folderPicker = await page.evaluate(() => typeof window.showDirectoryPicker === "function");
  if (folderPicker) {
    await expect(page.getByLabel("Salvar direto numa pasta do dispositivo (pergunta a cada download)")).toBeVisible();
  } else {
    await expect(page.getByText("Os arquivos vão para os downloads do navegador")).toBeVisible();
  }
});

test("downloads a playlist as a zip with tags and m3u8", async ({ page }) => {
  await browserDownloads(page);
  await page.goto("/");
  await inspect(page);
  const saved = page.waitForEvent("download");
  await page.getByRole("button", { name: "Baixar" }).click();
  const download = await saved;
  expect(download.suggestedFilename()).toBe(`${FOLDER}.zip`);
  const path = await download.path();
  const { readFileSync } = await import("node:fs");
  const zip = readFileSync(path).toString("latin1");
  for (const name of ["01. Perseus Ensemble - Faixa 1.mp3", "02. Perseus Ensemble - Faixa 2.mp3", "Argonautas.m3u8"]) {
    expect(zip).toContain(name);
  }
  expect(zip).toContain("ID3");
  expect(zip).toContain("#EXTINF:120,Perseus Ensemble - Faixa 1");

  await expect(page.getByText("2 baixadas", { exact: true })).toBeVisible();
  await expect(page.getByText("1 indisponível", { exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Salvar arquivo" })).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Tarefas desta sessão" })).toContainText("Concluída");
});

test("explains server limits in the user's language", async ({ page }) => {
  await page.unroute("**/api/plan");
  await mockServer(page, 400);
  await browserDownloads(page);
  await page.goto("/");
  await inspect(page);
  await page.getByRole("button", { name: "Baixar" }).click();
  await expect(page.getByRole("alert")).toContainText("A versão online baixa até 1.000 faixas por vez");
});

const PHONES = [
  { name: "phone-320", width: 320, height: 640 },
  { name: "phone-360", width: 360, height: 780 },
  { name: "phone-390", width: 390, height: 844 },
  { name: "phone-414", width: 414, height: 896 },
  { name: "tablet-768", width: 768, height: 1024 },
];

for (const viewport of PHONES) {
  test(`layout fits a ${viewport.name} screen`, async ({ page }) => {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    await page.goto("/");
    await page.screenshot({ path: `test-results/responsive-web/${test.info().project.name}-${viewport.name}-empty.png` });
    await inspect(page);
    await page.screenshot({ path: `test-results/responsive-web/${test.info().project.name}-${viewport.name}-release.png` });

    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    expect(overflow).toBeLessThanOrEqual(0);
    for (const control of [
      page.getByLabel("Link do SoundCloud"),
      page.getByRole("button", { name: "Baixar" }),
      page.getByRole("button", { name: "Ver faixas" }),
    ]) {
      await expect(control).toBeInViewport();
    }
    if (test.info().project.use.hasTouch) {
      for (const control of [page.getByRole("button", { name: "Baixar" }), page.getByRole("button", { name: "Ver faixas" })]) {
        const box = await control.boundingBox();
        expect(box?.height ?? 0).toBeGreaterThanOrEqual(44);
      }
    }
  });
}

test("has no serious accessibility violations on a phone", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  await inspect(page);
  const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa"]).analyze();
  const serious = results.violations.filter((item) => item.impact === "serious" || item.impact === "critical");
  expect(serious.map((item) => item.id)).toEqual([]);
});
