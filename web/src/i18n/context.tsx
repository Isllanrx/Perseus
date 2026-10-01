import { createContext, type ReactElement, type ReactNode, useContext, useEffect, useMemo, useState } from "react";

import { type Direction, direction, isLocale, type Locale, matchLocale, SOURCE_LOCALE } from "./locales";
import { createTranslator, type Translator } from "./translate";

export const LOCALE_KEY = "perseus.locale.v1";

export interface I18n extends Translator {
  dir: Direction;
  setLocale: (locale: Locale) => void;
  formatTime: (epochSeconds: number, withSeconds?: boolean) => string;
}

function build(locale: Locale, setLocale: (locale: Locale) => void): I18n {
  const translator = createTranslator(locale);
  const short = new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" });
  const long = new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
  return {
    ...translator,
    dir: direction(locale),
    setLocale,
    formatTime: (epochSeconds, withSeconds = false) =>
      (withSeconds ? long : short).format(new Date(epochSeconds * 1000)),
  };
}

const I18nContext = createContext<I18n>(build(SOURCE_LOCALE, () => undefined));

function initialLocale(): Locale {
  try {
    const stored = localStorage.getItem(LOCALE_KEY);
    if (isLocale(stored)) return stored;
  } catch {
  }
  return matchLocale(navigator.languages.length ? navigator.languages : [navigator.language]);
}

interface I18nProviderProps {
  children: ReactNode;
  locale?: Locale;
}

export function I18nProvider({ children, locale: forced }: I18nProviderProps): ReactElement {
  const [locale, setLocale] = useState<Locale>(() => forced ?? initialLocale());

  const value = useMemo(
    () =>
      build(locale, (next) => {
        setLocale(next);
        try {
          localStorage.setItem(LOCALE_KEY, next);
        } catch {
        }
      }),
    [locale],
  );

  useEffect(() => {
    document.documentElement.lang = locale;
    document.documentElement.dir = value.dir;
  }, [locale, value.dir]);

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18n {
  return useContext(I18nContext);
}
