import { expect, test } from "@playwright/test";

test.skip(({ browserName }) => browserName !== "chromium" || process.platform !== "win32", "baseline so no Chromium/Windows");

const PLAYLIST = "https://soundcloud.com/perseus/sets/argonautas";

test.beforeEach(async ({ page }) => {
  await page.setViewportSize({ width: 1366, height: 768 });
  await page.goto("/");
  await page.addStyleTag({ content: "*,*::before,*::after{animation:none!important;transition:none!important;caret-color:transparent!important}" });
});

test("estado vazio", async ({ page }) => {
  await expect(page).toHaveScreenshot("vazio.png", { fullPage: true, maxDiffPixelRatio: 0.01 });
});

test("playlist inspecionada", async ({ page }) => {
  await page.getByLabel("Link do SoundCloud").fill(PLAYLIST);
  await page.getByRole("button", { name: "Ver faixas" }).click();
  await expect(page.getByRole("heading", { name: "Argonautas" })).toBeVisible();
  await expect(page).toHaveScreenshot("playlist.png", { fullPage: true, maxDiffPixelRatio: 0.01 });
});

test("ajustes abertos", async ({ page }) => {
  await page.getByText("Ajustes").click();
  await expect(page.getByLabel("Qualidade")).toBeVisible();
  await expect(page.locator(".settings")).toHaveScreenshot("ajustes.png", { maxDiffPixelRatio: 0.01 });
});

test("layout da direita para a esquerda (arabe)", async ({ page }) => {
  await page.getByLabel("Idioma").selectOption("ar");
  await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
  await expect(page).toHaveScreenshot("rtl-ar.png", { fullPage: true, maxDiffPixelRatio: 0.01 });
});
