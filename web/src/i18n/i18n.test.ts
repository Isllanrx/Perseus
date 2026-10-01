import { describe, expect, it } from "vitest";

import { describeEvent, errorText, reasonText } from "./describe";
import { direction, LOCALES, matchLocale, MESSAGES } from "./locales";
import { ptBR } from "./messages/pt-BR";
import { createTranslator } from "./translate";
import type { Message } from "./types";

function placeholders(message: Message): Set<string> {
  const texts: string[] =
    typeof message === "string" ? [message] : Object.values(message).filter((text) => typeof text === "string");
  return new Set(texts.flatMap((text) => Array.from(text.matchAll(/\{(\w+)\}/g), (match) => match[1] ?? "")));
}

describe("dictionaries", () => {
  it("cover every locale offered in the picker", () => {
    expect(Object.keys(MESSAGES).sort()).toEqual(LOCALES.map((item) => item.code).sort());
  });

  it.each(LOCALES.map((item) => item.code))("%s only uses placeholders known to the source", (code) => {
    const messages = MESSAGES[code];
    for (const [key, source] of Object.entries(ptBR)) {
      const translated = messages[key as keyof typeof ptBR];
      expect(translated, key).toBeDefined();
      const allowed = placeholders(source);
      for (const name of placeholders(translated)) expect(allowed.has(name), `${code} ${key} {${name}}`).toBe(true);
      if (typeof translated !== "string") expect(translated.other, key).toBeTruthy();
    }
  });
});

describe("matchLocale", () => {
  it.each([
    [["pt-BR"], "pt-BR"],
    [["pt"], "pt-BR"],
    [["pt-PT"], "pt-PT"],
    [["pt-AO"], "pt-PT"],
    [["zh-Hans-CN"], "zh-CN"],
    [["zh-TW", "fr-CA"], "fr"],
    [["es-MX"], "es"],
    [["de-DE", "ru-RU"], "ru"],
    [["ar-EG"], "ar"],
    [["de-DE"], "en"],
    [[], "en"],
  ] as const)("%j -> %s", (preferences, expected) => {
    expect(matchLocale(preferences)).toBe(expected);
  });

  it("knows which languages are right-to-left", () => {
    expect(direction("ar")).toBe("rtl");
    expect(direction("hi")).toBe("ltr");
  });
});

describe("translator", () => {
  it("interpolates and pluralizes with CLDR rules", () => {
    const en = createTranslator("en");
    expect(en.t("release.progress", { finished: 1, count: 1 })).toBe("1 of 1 track");
    expect(en.t("release.progress", { finished: 2, count: 5 })).toBe("2 of 5 tracks");

    const ru = createTranslator("ru");
    expect(ru.t("release.more", { count: 3 })).toContain("3 трека");
    expect(ru.t("release.more", { count: 5 })).toContain("5 треков");

    const ar = createTranslator("ar");
    expect(ar.t("settings.peekLimit", { count: 2 })).toBe("، أول مقطعين");
  });

  it("does not group digits of track ids", () => {
    expect(createTranslator("pt-BR").t("log.track_failed", { track_id: 1234567890, reason: "x" })).toBe(
      "Faixa 1234567890 falhou: x",
    );
  });
});

describe("backend events and errors", () => {
  const fr = createTranslator("fr");

  it("translates availability reasons by code and keeps free text otherwise", () => {
    expect(reasonText(fr, "protegida por DRM", "drm")).toBe("protégé par DRM");
    expect(reasonText(fr, "timeout", null)).toBe("timeout");
    expect(reasonText(fr, null, null)).toBeNull();
  });

  it("builds activity lines from typed events", () => {
    const detail = { event: "track_unavailable", track_id: 7, reason: "protegida por DRM", reason_code: "drm" };
    expect(describeEvent(fr, detail, "fallback")).toBe("Titre 7 indisponible : protégé par DRM");
    expect(describeEvent(fr, { event: "watch_new_tracks", count: 2 }, "x")).toBe("2 titres nouveaux ou en attente");
    expect(describeEvent(fr, { event: "desconhecido" }, "texto original")).toBe("texto original");
    expect(describeEvent(fr, null, "texto original")).toBe("texto original");
  });

  it("uses dedicated messages for failed jobs when the code is known", () => {
    const failed = { event: "job_failed", code: "not_found", reason: "Recurso nao encontrado" };
    expect(describeEvent(fr, failed, "x")).toMatch(/^La tâche a échoué : Introuvable/);
    const unknown = { event: "job_failed", code: "transfer", reason: "tempo esgotado" };
    expect(describeEvent(fr, unknown, "x")).toBe("La tâche a échoué : tempo esgotado");
  });

  it("maps error codes with aliases and a generic fallback", () => {
    expect(errorText(fr, "untrusted_url")).toBe(errorText(fr, "invalid_url"));
    expect(errorText(fr, "transfer")).toBe(fr.t("error.generic"));
    expect(errorText(fr, null)).toBe(fr.t("error.generic"));
  });
});
