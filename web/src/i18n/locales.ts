import { ar } from "./messages/ar";
import { bn } from "./messages/bn";
import { en } from "./messages/en";
import { es } from "./messages/es";
import { fr } from "./messages/fr";
import { hi } from "./messages/hi";
import { id } from "./messages/id";
import { type Messages, ptBR } from "./messages/pt-BR";
import { ptPT } from "./messages/pt-PT";
import { ru } from "./messages/ru";
import { zhCN } from "./messages/zh-CN";

export const LOCALES = [
  { code: "en", name: "English", dir: "ltr" },
  { code: "es", name: "Español", dir: "ltr" },
  { code: "zh-CN", name: "中文（简体）", dir: "ltr" },
  { code: "hi", name: "हिन्दी", dir: "ltr" },
  { code: "fr", name: "Français", dir: "ltr" },
  { code: "pt-BR", name: "Português (Brasil)", dir: "ltr" },
  { code: "pt-PT", name: "Português (Portugal)", dir: "ltr" },
  { code: "ar", name: "العربية", dir: "rtl" },
  { code: "bn", name: "বাংলা", dir: "ltr" },
  { code: "ru", name: "Русский", dir: "ltr" },
  { code: "id", name: "Bahasa Indonesia", dir: "ltr" },
] as const;

export type Locale = (typeof LOCALES)[number]["code"];
export type Direction = "ltr" | "rtl";

export const DEFAULT_LOCALE: Locale = "en";
export const SOURCE_LOCALE: Locale = "pt-BR";

export const MESSAGES: Record<Locale, Messages> = {
  en,
  es,
  "zh-CN": zhCN,
  hi,
  fr,
  "pt-BR": ptBR,
  "pt-PT": ptPT,
  ar,
  bn,
  ru,
  id,
};

const CODES: readonly string[] = LOCALES.map((locale) => locale.code);

export function isLocale(value: unknown): value is Locale {
  return typeof value === "string" && CODES.includes(value);
}

export function direction(locale: Locale): Direction {
  return LOCALES.find((item) => item.code === locale)?.dir ?? "ltr";
}

const PT_PT_REGIONS = new Set(["pt", "ao", "mz", "cv", "gw", "st", "tl", "mo", "ch", "lu"]);

export function matchLocale(preferences: readonly string[]): Locale {
  for (const preference of preferences) {
    const [language = "", ...rest] = preference.toLowerCase().split(/[-_]/);
    const region = rest.find((part) => part.length === 2) ?? "";
    const exact = LOCALES.find((item) => item.code.toLowerCase() === preference.toLowerCase());
    if (exact) return exact.code;
    if (language === "pt") return PT_PT_REGIONS.has(region) ? "pt-PT" : "pt-BR";
    if (language === "zh" && !rest.includes("hant") && !["tw", "hk", "mo"].includes(region)) return "zh-CN";
    const byLanguage = LOCALES.find((item) => item.code === language);
    if (byLanguage) return byLanguage.code;
  }
  return DEFAULT_LOCALE;
}
