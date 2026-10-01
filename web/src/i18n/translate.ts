import { type Locale, MESSAGES } from "./locales";
import type { MessageKey } from "./messages/pt-BR";
import type { Message, Plural, Vars } from "./types";

export type Translate = (key: MessageKey, vars?: Vars) => string;

export interface Translator {
  locale: Locale;
  t: Translate;
  has: (key: string) => key is MessageKey;
  formatNumber: (value: number) => string;
}

function selectPlural(message: Plural, rules: Intl.PluralRules, count: number): string {
  const form = rules.select(count);
  return message[form] ?? message.other;
}

export function createTranslator(locale: Locale): Translator {
  const messages = MESSAGES[locale];
  const rules = new Intl.PluralRules(locale);
  const numbers = new Intl.NumberFormat(locale, { maximumFractionDigits: 1, useGrouping: false });
  const formatNumber = (value: number): string => numbers.format(value);

  const interpolate = (template: string, vars: Vars): string =>
    template.replace(/\{(\w+)\}/g, (match, name: string) => {
      const value = vars[name];
      if (value === undefined) return match;
      return typeof value === "number" ? formatNumber(value) : value;
    });

  const has = (key: string): key is MessageKey => key in messages;

  const t: Translate = (key, vars = {}) => {
    const message: Message = messages[key];
    const template =
      typeof message === "string" ? message : selectPlural(message, rules, Number(vars.count ?? 0));
    return interpolate(template, vars);
  };

  return { locale, t, has, formatNumber };
}
