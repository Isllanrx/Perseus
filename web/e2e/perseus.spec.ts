import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

const PLAYLIST = "https://soundcloud.com/perseus/sets/argonautas";
const SLOW_PLAYLIST = "https://soundcloud.com/perseus/sets/lenta";

async function inspect(page: Page, url: string): Promise<void> {
  await page.getByLabel("Link do SoundCloud").fill(url);
  await page.getByRole("button", { name: "Ver faixas" }).click();
}

test.beforeEach(async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  await page.goto("/");
  test.info().annotations.push({ type: "console-errors", description: String(errors.length) });
});

test("empty state presents the brand and the call to action", async ({ page }) => {
  await expect(page.getByRole("heading", { name: "Cole um link do SoundCloud" })).toBeVisible();
  await expect(page.getByText("Desenvolvido por Isllan Toso")).toBeVisible();
  await expect(page.getByRole("button", { name: "Baixar" })).toBeDisabled();
});

test("inspects a playlist and flags DRM tracks", async ({ page }) => {
  await inspect(page, PLAYLIST);
  await expect(page.getByRole("heading", { name: "Argonautas" })).toBeVisible();
  await expect(page.getByRole("listitem").filter({ hasText: "Faixa 12" })).toContainText("protegida por DRM");
});

test("searches by name and opens the chosen result", async ({ page }) => {
  await page.getByLabel("Link do SoundCloud").fill("argonautas");
  await page.getByRole("button", { name: "Buscar" }).click();
  await expect(page.getByRole("heading", { name: "Resultados para “argonautas”" })).toBeVisible();
  await page.getByRole("button", { name: /Argonautas/ }).click();
  await expect(page.getByRole("heading", { name: "Argonautas" })).toBeVisible();
  await expect(page.getByLabel("Link do SoundCloud")).toHaveValue(PLAYLIST);
});

test("rejects links outside SoundCloud with a clear message", async ({ page }) => {
  await inspect(page, "https://evil.example/a/b");
  await expect(page.getByRole("alert")).toContainText("Esse link não é do SoundCloud");
});

test("downloads a playlist with live progress", async ({ page }) => {
  await inspect(page, PLAYLIST);
  await page.getByRole("button", { name: "Baixar" }).click();
  await expect(page.getByText("12 de 12 faixas", { exact: true })).toBeVisible({ timeout: 15_000 });
  await expect(page.getByText("11 baixadas", { exact: true })).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Tarefas desta sessão" })).toContainText("Concluída");
  await page.getByText("Registro de atividade").click();
  await expect(page.locator(".activity-list")).toContainText("Concluída: 01. Perseus Ensemble - Faixa 01.mp3");
  await expect(page.locator(".activity-list")).toContainText("Faixa 1012 indisponível: protegida por DRM");
});

test("cancels a running download", async ({ page }) => {
  await inspect(page, SLOW_PLAYLIST);
  await page.getByRole("button", { name: "Baixar" }).click();
  await page.getByRole("button", { name: "Cancelar" }).click();
  await expect(page.getByRole("navigation", { name: "Tarefas desta sessão" })).toContainText("Cancelada", {
    timeout: 10_000,
  });
});

test("has no serious accessibility violations", async ({ page }) => {
  await inspect(page, PLAYLIST);
  await expect(page.getByRole("heading", { name: "Argonautas" })).toBeVisible();
  const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21aa"]).analyze();
  const serious = results.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
  expect(serious.map((v) => `${v.id}: ${v.nodes.length}`)).toEqual([]);
});

test("switches language, including a right-to-left layout", async ({ page }) => {
  await inspect(page, PLAYLIST);
  await page.getByLabel("Idioma").selectOption("ar");
  await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
  await expect(page.getByRole("button", { name: "تنزيل" })).toBeVisible();
  await expect(page.getByRole("listitem").filter({ hasText: "Faixa 12" })).toContainText("محمي بإدارة الحقوق الرقمية");

  const sidebar = await page.locator(".sidebar").boundingBox();
  const main = await page.locator(".main").boundingBox();
  expect(sidebar && main && sidebar.x > main.x).toBe(true);
  expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(0);
  const results = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa", "wcag21aa"]).analyze();
  expect(results.violations.filter((v) => v.impact === "serious" || v.impact === "critical").map((v) => v.id)).toEqual([]);
  await page.screenshot({ path: "test-results/responsive/rtl-ar.png" });

  await page.reload();
  await expect(page.getByLabel("اللغة")).toHaveValue("ar");
  await page.getByLabel("اللغة").selectOption("hi");
  await expect(page.locator("html")).toHaveAttribute("dir", "ltr");
  await expect(page.getByRole("heading", { name: "SoundCloud लिंक पेस्ट करें" })).toBeVisible();
});

const VIEWPORTS = [
  { name: "desktop-1920", width: 1920, height: 1080 },
  { name: "desktop-1440", width: 1440, height: 900 },
  { name: "notebook-1366", width: 1366, height: 768 },
  { name: "notebook-1280", width: 1280, height: 720 },
  { name: "notebook-1024", width: 1024, height: 768 },
  { name: "compact-768", width: 768, height: 1024 },
];

for (const viewport of VIEWPORTS) {
  test(`layout holds at ${viewport.name}`, async ({ page }) => {
    await page.setViewportSize({ width: viewport.width, height: viewport.height });
    await page.goto("/");
    await page.screenshot({ path: `test-results/responsive/${viewport.name}-empty.png` });

    await inspect(page, PLAYLIST);
    await expect(page.getByRole("heading", { name: "Argonautas" })).toBeVisible();
    await page.screenshot({ path: `test-results/responsive/${viewport.name}-release.png` });

    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    expect(overflow).toBeLessThanOrEqual(0);
    for (const control of [
      page.getByLabel("Link do SoundCloud"),
      page.getByRole("button", { name: "Baixar" }),
      page.getByText("Ajustes"),
    ]) {
      await expect(control).toBeInViewport();
    }
  });
}
