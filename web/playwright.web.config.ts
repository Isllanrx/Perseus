import { defineConfig, devices } from "@playwright/test";

const PORT = 4174;
export default defineConfig({
  testDir: "./e2e-web",
  outputDir: "test-results/web",
  fullyParallel: false,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  reporter: [["list"], ["html", { open: "never", outputFolder: "playwright-report-web" }]],
  use: {
    baseURL: `http://localhost:${String(PORT)}`,
    locale: "pt-BR",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    acceptDownloads: true,
  },
  projects: [
    { name: "web-chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "web-android", use: { ...devices["Pixel 7"] } },
    { name: "web-iphone", use: { ...devices["iPhone 14"] } },
  ],
  webServer: {
    command: `npm run dev:web -- --port ${String(PORT)}`,
    url: `http://localhost:${String(PORT)}/`,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
