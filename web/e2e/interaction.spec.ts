import { expect, test, type Page } from "@playwright/test";

const PLAYLIST = "https://soundcloud.com/perseus/sets/argonautas";
const SLOW_PLAYLIST = "https://soundcloud.com/perseus/sets/lenta";

async function inspect(page: Page, url: string): Promise<void> {
  await page.getByLabel("Link do SoundCloud").fill(url);
  await page.getByRole("button", { name: "Ver faixas" }).click();
}

test.beforeEach(async ({ page }) => {
  await page.goto("/");
});

test.describe("teclado", () => {
  test("Enter no campo inicia o download sem mouse", async ({ page }) => {
    const input = page.getByLabel("Link do SoundCloud");
    await input.focus();
    await page.keyboard.type(PLAYLIST);
    await page.keyboard.press("Enter");
    await expect(page.getByText("12 de 12 faixas", { exact: true })).toBeVisible({ timeout: 15_000 });
  });

  test("Tab percorre os controles na ordem visual e o foco fica visivel", async ({ page }) => {
    await page.getByLabel("Link do SoundCloud").fill(PLAYLIST);
    await page.getByLabel("Link do SoundCloud").focus();
    const order: string[] = [];
    for (let step = 0; step < 4; step += 1) {
      await page.keyboard.press("Tab");
      const focused = page.locator(":focus");
      await expect(focused).toBeVisible();
      order.push((await focused.getAttribute("title")) ?? (await focused.innerText()).trim());
    }
    expect(order[0]).toMatch(/Colar|área de transferência/i);
    expect(order[1]).toMatch(/Ver faixas/);
  });

  test("modos trocam com as setas e os ajustes abrem pelo teclado", async ({ page }) => {
    await page.getByRole("radio", { name: "Playlist inteira" }).focus();
    await page.keyboard.press("ArrowRight");
    await expect(page.getByRole("radio", { name: "Só esta faixa" })).toBeChecked();

    await page.getByText("Ajustes").focus();
    await page.keyboard.press("Enter");
    await expect(page.getByLabel("Qualidade")).toBeVisible();
  });
});

test.describe("formularios", () => {
  test("ajustes limitam valores fora da faixa", async ({ page }) => {
    await page.getByText("Ajustes").click();
    const workers = page.getByLabel(/Downloads simultâneos/);
    await workers.focus();
    await page.keyboard.press("End");
    await expect(workers).toHaveValue("16");
    await page.keyboard.press("Home");
    await expect(workers).toHaveValue("1");

    const limit = page.getByLabel("Baixar só as primeiras");
    await limit.fill("-5");
    await expect(limit).toHaveValue("1");
    await limit.fill("");
    await expect(limit).toHaveValue("");
  });

  test("campo vazio mantem as acoes desabilitadas", async ({ page }) => {
    await expect(page.getByRole("button", { name: "Baixar" })).toBeDisabled();
    await page.getByLabel("Link do SoundCloud").fill("   ");
    await expect(page.getByRole("button", { name: "Baixar" })).toBeDisabled();
  });
});

test("navegacao pelo historico alterna entre tarefas", async ({ page }) => {
  await inspect(page, PLAYLIST);
  await page.getByRole("button", { name: "Baixar" }).click();
  await expect(page.getByText("12 de 12 faixas", { exact: true })).toBeVisible({ timeout: 15_000 });

  await inspect(page, SLOW_PLAYLIST);
  await page.getByRole("button", { name: "Baixar" }).click();
  const history = page.getByRole("navigation", { name: "Tarefas desta sessão" });
  await expect(history.getByRole("button")).toHaveCount(2);

  await history.getByRole("button", { name: /Argonautas/ }).click();
  await expect(page.getByRole("heading", { name: "Argonautas" })).toBeVisible();
  await history.getByRole("button", { name: /Lenta/ }).click();
  await expect(page.getByRole("heading", { name: "Lenta" })).toBeVisible();
  await page.getByRole("button", { name: "Cancelar" }).click();
});

test("fluxo completo nao faz nenhuma requisicao externa (offline e privacidade)", async ({ page }) => {
  const external: string[] = [];
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (!["localhost", "127.0.0.1"].includes(url.hostname) && url.protocol.startsWith("http")) {
      external.push(request.url());
    }
  });
  await inspect(page, PLAYLIST);
  await page.getByRole("button", { name: "Baixar" }).click();
  await expect(page.getByText("12 de 12 faixas", { exact: true })).toBeVisible({ timeout: 15_000 });
  await page.getByLabel("Idioma").selectOption("en");
  expect(external).toEqual([]);
});

test("titulo com HTML e exibido como texto (XSS)", async ({ page }) => {
  await page.getByLabel("Link do SoundCloud").fill("<img src=x onerror=window.__xss=1>");
  await page.getByRole("button", { name: "Buscar" }).click();
  await expect(page.getByRole("heading", { name: /Resultados para/ })).toContainText("<img src=x onerror=window.__xss=1>");
  expect(await page.evaluate(() => (window as unknown as { __xss?: number }).__xss)).toBeUndefined();
  await expect(page.locator("img[src='x']")).toHaveCount(0);
});
