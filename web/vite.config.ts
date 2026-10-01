import { readFileSync } from "node:fs";

import react from "@vitejs/plugin-react";
import { type Plugin, defineConfig } from "vite";

const DESCRIPTION =
  "Baixe faixas, álbuns e playlists públicas do SoundCloud com capa e metadados, direto no navegador. Sem login.";

const MANIFEST = {
  name: "Perseus",
  short_name: "Perseus",
  description: DESCRIPTION,
  start_url: "/",
  scope: "/",
  display: "browser",
  background_color: "#000000",
  theme_color: "#000000",
  icons: [
    { src: "/icon-192.png", sizes: "192x192", type: "image/png" },
    { src: "/icon-512.png", sizes: "512x512", type: "image/png" },
  ],
};

function webShell(): Plugin {
  const manifest = JSON.stringify(MANIFEST, null, 2);
  return {
    name: "perseus-web-shell",
    transformIndexHtml: (html) =>
      html
        .replace('<html lang="pt-BR">', '<html lang="pt-BR" data-platform="web">')
        .replace(
          '<meta name="viewport" content="width=device-width, initial-scale=1" />',
          '<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />',
        )
        .replace(
          "</head>",
          `    <meta name="description" content="${DESCRIPTION}" />
    <meta name="theme-color" content="#000000" />
    <link rel="manifest" href="/manifest.webmanifest" />
  </head>`,
        ),
    configureServer(server) {
      server.middlewares.use("/manifest.webmanifest", (_request, response) => {
        response.setHeader("content-type", "application/manifest+json");
        response.end(manifest);
      });
    },
    generateBundle() {
      this.emitFile({ type: "asset", fileName: "manifest.webmanifest", source: manifest });
      this.emitFile({
        type: "asset",
        fileName: "icon-512.png",
        source: readFileSync(new URL("../src-tauri/icons/icon.png", import.meta.url)),
      });
    },
  };
}

export default defineConfig(({ mode }) => {
  const web = mode === "web";
  return {
    plugins: [react(), ...(web ? [webShell()] : [])],
    clearScreen: false,
    server: web
      ? { port: 5173, strictPort: true, proxy: { "/api": "http://127.0.0.1:3000" } }
      : { port: 1420, strictPort: true },
    envPrefix: ["VITE_", "TAURI_ENV_"],
    build: {
      outDir: "dist",
      emptyOutDir: true,
      assetsInlineLimit: 0,
      target: "es2023",
      sourcemap: false,
    },
  };
});
