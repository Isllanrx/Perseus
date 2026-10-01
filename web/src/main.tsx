import "@fontsource-variable/hanken-grotesk";
import "@fontsource/cormorant-garamond/500.css";
import "@fontsource/cormorant-garamond/600.css";
import "./styles.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { I18nProvider } from "./i18n/context";

const root = document.getElementById("root");
if (!root) throw new Error("Elemento #root ausente em index.html");

if (import.meta.env.MODE === "e2e") {
  const { installMockBackend } = await import("./e2e/mockBackend");
  installMockBackend();
}

createRoot(root).render(
  <StrictMode>
    <I18nProvider>
      <App />
    </I18nProvider>
  </StrictMode>,
);
