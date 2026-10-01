import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    restoreMocks: true,
    coverage: {
      provider: "v8",
      include: ["src/**/*.{ts,tsx}"],
      exclude: ["src/main.tsx", "src/test/**", "src/e2e/**", "src/**/*.test.{ts,tsx}", "src/vite-env.d.ts", "src/types.ts"],
      reporter: ["text", "html", "lcov"],
      thresholds: { lines: 80, functions: 80, statements: 80, branches: 70 },
    },
  },
});
