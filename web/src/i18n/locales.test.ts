import { describe, expect, it } from "vitest";

import { DEFAULT_LOCALE, LOCALES, SOURCE_LOCALE, direction, isLocale, matchLocale } from "./locales";

describe("locales", () => {
  it("writes source strings in Brazilian Portuguese and falls back to English", () => {
    expect(SOURCE_LOCALE).toBe("pt-BR");
    expect(DEFAULT_LOCALE).toBe("en");
  });

  it("recognizes only offered locale codes", () => {
    for (const { code } of LOCALES) expect(isLocale(code)).toBe(true);
    for (const value of ["xx", "PT-BR", "", "zh", 42, null, undefined, { code: "en" }]) {
      expect({ value, locale: isLocale(value) }).toEqual({ value, locale: false });
    }
  });

  it("knows text direction and defaults to left-to-right", () => {
    expect(direction("ar")).toBe("rtl");
    expect(direction("en")).toBe("ltr");
    expect(direction("xx" as never)).toBe("ltr");
  });

  it.each([
    [["pt-BR"], "pt-BR"],
    [["PT-br"], "pt-BR"],
    [["pt_PT"], "pt-PT"],
    [["pt-AO"], "pt-PT"],
    [["pt"], "pt-BR"],
    [["zh"], "zh-CN"],
    [["zh-Hans-CN"], "zh-CN"],
    [["zh-TW", "fr-CA"], "fr"],
    [["zh-Hant", "es-MX"], "es"],
    [["en-US"], "en"],
    [["xx", "ru-RU"], "ru"],
    [["ID"], "id"],
    [[], "en"],
    [["xx-YY"], "en"],
  ])("matches %j to %s", (preferences, expected) => {
    expect(matchLocale(preferences)).toBe(expected);
  });
});
