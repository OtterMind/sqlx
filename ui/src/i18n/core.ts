/** Languages the bundled catalogues cover; adding one means adding a file in ./locales. */
export type Locale = "en-US" | "zh-CN";

export const localeNames: Record<Locale, string> = {
  "en-US": "English",
  "zh-CN": "简体中文",
};

const storageKey = "sqlx.ui.locale";
const fallback: Locale = "en-US";
let active: Locale | undefined;

function fromBrowser(): Locale | undefined {
  for (const tag of navigator.languages ?? [navigator.language]) {
    const value = tag.toLowerCase();
    if (value.startsWith("zh")) return "zh-CN";
    if (value.startsWith("en")) return "en-US";
  }
  return undefined;
}

function isLocale(value: unknown): value is Locale {
  return value === "en-US" || value === "zh-CN";
}

/** The stored choice wins, then the browser language, then English. */
export function detectLocale(): Locale {
  try {
    const saved = localStorage.getItem(storageKey);
    if (isLocale(saved)) return saved;
  } catch {
    // Language switching still works when browser storage is unavailable.
  }
  return fromBrowser() ?? fallback;
}

export function currentLocale(): Locale {
  active ??= detectLocale();
  return active;
}

export function storeLocale(locale: Locale): void {
  try {
    localStorage.setItem(storageKey, locale);
  } catch {
    // The choice still applies to this page.
  }
}

export function activateLocale(locale: Locale): void {
  active = locale;
  document.documentElement.lang = locale;
}

export function localeFromTag(tag: string): Locale | undefined {
  return isLocale(tag) ? tag : undefined;
}

/** Parameters are substituted as {name}; a missing value is left untouched so bugs stay visible. */
export function format(template: string, params?: Record<string, string | number>): string {
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}
