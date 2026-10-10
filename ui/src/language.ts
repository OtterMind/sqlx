import { locales, currentLocale, setLocale, t } from "./i18n";
import type { Locale, TranslationKey } from "./i18n";

/**
 * The topbar language picker. Switching stores the choice and re-renders the current page;
 * the theme toggle stays a plain light/dark switch.
 */
export function initializeLanguage(): void {
  const toggle = document.querySelector<HTMLButtonElement>("#language-toggle");
  if (!toggle) return;
  toggle.classList.add("language-toggle");
  toggle.setAttribute("aria-haspopup", "menu");
  toggle.setAttribute("aria-expanded", "false");
  const label = () => {
    toggle.title = t("language.switch");
    toggle.setAttribute("aria-label", t("language.switch"));
  };
  label();

  let menu: HTMLElement | undefined;
  const close = () => {
    menu?.remove();
    menu = undefined;
    toggle.setAttribute("aria-expanded", "false");
  };
  toggle.addEventListener("click", event => {
    event.stopPropagation();
    if (menu) {
      close();
      return;
    }
    const current = currentLocale();
    menu = document.createElement("div");
    menu.className = "choice-menu-options language-menu";
    menu.setAttribute("role", "menu");
    for (const locale of Object.keys(locales) as Locale[]) {
      const item = document.createElement("button");
      item.type = "button";
      item.className = "choice-menu-item";
      item.setAttribute("role", "menuitemradio");
      item.setAttribute("aria-checked", String(locale === current));
      item.textContent = locales[locale];
      item.onclick = () => {
        close();
        if (locale === current) return;
        setLocale(locale);
        label();
      };
      menu.append(item);
    }
    document.body.append(menu);
    const box = toggle.getBoundingClientRect();
    menu.style.position = "fixed";
    menu.style.top = `${Math.round(box.bottom + 6)}px`;
    menu.style.right = `${Math.round(window.innerWidth - box.right)}px`;
    menu.style.left = "auto";
    toggle.setAttribute("aria-expanded", "true");
    menu.querySelector<HTMLButtonElement>('[aria-checked="true"]')?.focus();
  });
  document.addEventListener("click", close);
  document.addEventListener("keydown", event => {
    if (event.key === "Escape" && menu) {
      close();
      toggle.focus();
    }
  });
}

/**
 * Localizes the static shell markup: every element marked with `data-i18n` takes its text from
 * the catalogue, or the attribute named by `data-i18n-attr` when the string is an attribute.
 */
export function initializeShellText(): void {
  for (const node of document.querySelectorAll<HTMLElement>("[data-i18n]")) {
    const translated = t(node.dataset.i18n as TranslationKey);
    const attribute = node.dataset.i18nAttr;
    if (attribute) node.setAttribute(attribute, translated);
    else node.textContent = translated;
  }
}
