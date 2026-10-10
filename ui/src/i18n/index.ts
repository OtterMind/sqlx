import { activateLocale, currentLocale, detectLocale, format, localeNames, storeLocale } from "./core";
import type { Locale } from "./core";
import english from "./locales/en-US.json";
import chinese from "./locales/zh-CN.json";

export type { Locale } from "./core";
export { localeNames } from "./core";

type Params = Record<string, string | number>;
/** English is the source catalogue: every key and its parameters are typed from it. */
export type TranslationKey = keyof typeof english;

const catalogues: Record<Locale, Record<string, string>> = { "en-US": english, "zh-CN": chinese };

/** Marks a literal as translatable without translating it now (module-level tables). */
export const key = (value: TranslationKey): TranslationKey => value;

export function t(translation: TranslationKey, params?: Params): string {
  const catalogue = catalogues[currentLocale()];
  return format(catalogue[translation] ?? english[translation] ?? translation, params);
}

/** Applies a locale, remembers it for the next visit and lets the app re-render itself. */
/** Plural-aware lookup: the `.one` variant is used when the count is exactly one. */
export function tn(one: TranslationKey, other: TranslationKey, count: number, params?: Params): string {
  return t(count === 1 ? one : other, { count, ...params });
}

export function setLocale(locale: Locale): void {
  activateLocale(locale);
  storeLocale(locale);
  document.dispatchEvent(new CustomEvent("sqlx:locale"));
}

export { currentLocale, detectLocale, localeNames as locales };
