import { key, t } from "./i18n";
import type { TranslationKey } from "./i18n";
import { navigate } from "./navigation";

export function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className = "",
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}
export function button(text: string, className = "button"): HTMLButtonElement {
  const node = element("button", className, text);
  node.type = "button";
  return node;
}
export function svgIcon(path: string): SVGSVGElement {
  const icon = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  for (const [name, value] of Object.entries({
    viewBox: "0 0 24 24",
    width: "16",
    height: "16",
    fill: "none",
    stroke: "currentColor",
    "stroke-width": "1.8",
    "stroke-linecap": "round",
    "stroke-linejoin": "round",
    "aria-hidden": "true",
  }))
    icon.setAttribute(name, value);
  const line = document.createElementNS(icon.namespaceURI, "path");
  line.setAttribute("d", path);
  icon.append(line);
  return icon;
}

/** Matches Chat2DB's result toolbar: 24px hit area and 16px line icon. */
export function resultIcon(label: string, path: string): HTMLButtonElement {
  const control = button("", "result-icon");
  control.append(svgIcon(path));
  control.title = label;
  control.setAttribute("aria-label", label);
  return control;
}
export function choiceMenu(
  label: string,
  choices: [string, string][],
  onChange: () => void,
  signal: AbortSignal,
) {
  const container = element("div", "choice-menu");
  const trigger = button("", "choice-menu-trigger");
  trigger.append(svgIcon("m7 10 5 5 5-5"));
  trigger.setAttribute("aria-label", label);
  trigger.setAttribute("aria-haspopup", "menu");
  trigger.setAttribute("aria-expanded", "false");
  const menu = element("div", "choice-menu-options");
  menu.setAttribute("role", "menu");
  menu.setAttribute("aria-label", label);
  menu.hidden = true;
  let value = choices[0][0];
  const items = choices.map(([value, text]) => {
    const item = button(text, "choice-menu-item");
    item.setAttribute("role", "menuitemradio");
    item.tabIndex = -1;
    item.onclick = () => {
      setValue(value);
      close(true);
      onChange();
    };
    menu.append(item);
    return { value, text, item };
  });
  function setValue(next: string) {
    value = next;
    for (const entry of items) {
      entry.item.setAttribute("aria-checked", String(entry.value === value));
      if (entry.value === value) trigger.title = `${label}: ${entry.text}`;
    }
  }
  function close(focus = false) {
    menu.hidden = true;
    trigger.setAttribute("aria-expanded", "false");
    if (focus) trigger.focus();
  }
  function open() {
    menu.hidden = false;
    trigger.setAttribute("aria-expanded", "true");
    items.find((entry) => entry.value === value)!.item.focus();
  }
  trigger.onclick = () => (menu.hidden ? open() : close());
  trigger.onkeydown = (event) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      open();
    }
  };
  menu.onkeydown = (event) => {
    const index = items.findIndex(
      (entry) => entry.item === document.activeElement,
    );
    const next =
      event.key === "ArrowDown"
        ? (index + 1) % items.length
        : event.key === "ArrowUp"
          ? (index + items.length - 1) % items.length
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? items.length - 1
              : undefined;
    if (next !== undefined) {
      event.preventDefault();
      items[next].item.focus();
    } else if (event.key === "Escape") {
      event.preventDefault();
      close(true);
    } else if (event.key === "Tab") close();
  };
  const outside = (event: PointerEvent) => {
    if (event.target instanceof Node && !container.contains(event.target))
      close();
  };
  document.addEventListener("pointerdown", outside);
  signal.addEventListener(
    "abort",
    () => document.removeEventListener("pointerdown", outside),
    { once: true },
  );
  container.append(trigger, menu);
  setValue(value);
  return {
    element: container,
    get value() {
      return value;
    },
    set value(next: string) {
      setValue(next);
    },
  };
}
/** Line icons shared by icon-only card actions, keyed by the action they perform. */
export const cardIcons = {
  edit: "M4 20h4L19 9a2.1 2.1 0 0 0-3-3L5 17v3M14.5 6.5l3 3",
  remove: "M6 6l12 12M18 6 6 18",
  drag: "M9 6h.01M9 12h.01M9 18h.01M15 6h.01M15 12h.01M15 18h.01",
  plug: "M9 3v6M15 3v6M6 9h12v2a6 6 0 0 1-12 0V9M12 17v4",
  play: "M8 5.4v13.2L19 12z",
  refresh: "M4 12a8 8 0 0 1 13.6-5.7M20 12a8 8 0 0 1-13.6 5.7M18 3v4h-4M6 21v-4h4",
  more: "M6 12h.01M12 12h.01M18 12h.01",
} as const;

/** A button whose icon and label sit on one line. */
export function iconLabelButton(label: string, icon: string, className = "button secondary"): HTMLButtonElement {
  const control = button("", className);
  control.append(svgIcon(icon), element("span", "", label));
  return control;
}

/**
 * Icon actions for a list or grid card: hidden until the card is hovered or focused, so the
 * action set never competes with the card's own content.
 */
export function cardActions(
  host: HTMLElement,
  actions: { label: string; icon: string; className?: string; onClick: () => void }[],
  linger = 1600,
  signal?: AbortSignal,
): HTMLElement {
  const tools = element("div", "card-actions");
  tools.append(...actions.map(action => {
    const control = resultIcon(action.label, action.icon);
    if (action.className) control.classList.add(action.className);
    control.onclick = action.onClick;
    return control;
  }));
  const hidden = (value: boolean) => tools.classList.toggle("card-actions-hidden", value);
  let timer = 0;
  hidden(true);
  host.addEventListener("pointerenter", () => {
    hidden(false); window.clearTimeout(timer);
    timer = window.setTimeout(() => hidden(true), linger);
  });
  host.addEventListener("focusin", () => { window.clearTimeout(timer); hidden(false); });
  host.addEventListener("focusout", () => hidden(true));
  signal?.addEventListener("abort", () => window.clearTimeout(timer), { once: true });
  // The card's own link should not swallow the icon clicks.
  tools.addEventListener("click", event => event.stopPropagation());
  if (host instanceof HTMLAnchorElement) {
    host.addEventListener("click", event => {
      const target = event.target as HTMLElement;
      if (target.closest(".card-actions")) return;
      const link = target.closest<HTMLAnchorElement>("a[href]");
      if (!link) return;
      event.preventDefault();
      navigate(link.getAttribute("href")!);
    });
  }
  return tools;
}

/** A short-lived message under a card or form, cleared once it has been read. */
export function inlineNotice(host: HTMLElement, text: string, error = false, linger = 4000): void {
  const node = element("p", error ? "feedback error inline-notice" : "feedback inline-notice", text);
  host.append(node);
  window.setTimeout(() => node.remove(), linger);
}

export interface MenuItem {
  label: string;
  danger?: boolean;
  onClick: () => void;
}

/** An icon trigger that opens a small menu anchored under itself; Escape and outside clicks close it. */
export function overflowMenu(trigger: HTMLButtonElement, items: MenuItem[], signal?: AbortSignal): void {
  trigger.setAttribute("aria-haspopup", "menu");
  trigger.setAttribute("aria-expanded", "false");
  let menu: HTMLElement | undefined;
  const close = () => {
    menu?.remove();
    menu = undefined;
    trigger.setAttribute("aria-expanded", "false");
  };
  trigger.addEventListener("click", event => {
    event.stopPropagation();
    if (menu) {
      close();
      return;
    }
    menu = element("div", "menu");
    menu.setAttribute("role", "menu");
    for (const item of items) {
      const control = element("button", item.danger ? "menu-item danger" : "menu-item");
      control.type = "button";
      control.setAttribute("role", "menuitem");
      control.textContent = item.label;
      control.onclick = () => { close(); item.onClick(); };
      menu.append(control);
    }
    document.body.append(menu);
    const box = trigger.getBoundingClientRect();
    menu.style.position = "fixed";
    menu.style.top = `${Math.round(box.bottom + 6)}px`;
    menu.style.right = `${Math.round(window.innerWidth - box.right)}px`;
    trigger.setAttribute("aria-expanded", "true");
    menu.querySelector<HTMLButtonElement>("button")?.focus();
  });
  document.addEventListener("click", close);
  document.addEventListener("keydown", event => { if (event.key === "Escape" && menu) { close(); trigger.focus(); } });
  signal?.addEventListener("abort", close, { once: true });
}

/** Modal confirmation; resolves true only when the user confirms. */
export function confirmDialog(options: { title: string; body?: string; confirmLabel: string; danger?: boolean; signal?: AbortSignal }): Promise<boolean> {
  return new Promise(resolve => {
    const dialog = element("dialog", "analytics-dialog");
    dialog.setAttribute("aria-label", options.title);
    // One primary action per dialog, in the theme colour; the title already says what it does.
    const form = element("form"), confirm = button(options.confirmLabel), cancel = button(t("common.cancel"), "button secondary");
    let answer = false;
    cancel.onclick = () => dialog.close();
    form.onsubmit = event => { event.preventDefault(); answer = true; dialog.close(); };
    confirm.type = "submit";
    const actions = element("div", "dialog-actions");
    actions.append(cancel, confirm);
    form.append(heading(options.title));
    if (options.body) form.append(element("p", "muted", options.body));
    form.append(actions);
    dialog.append(form);
    document.body.append(dialog);
    const abort = () => dialog.close();
    options.signal?.addEventListener("abort", abort, { once: true });
    dialog.addEventListener("close", () => {
      options.signal?.removeEventListener("abort", abort);
      dialog.remove();
      resolve(answer);
    }, { once: true });
    dialog.showModal();
  });
}

export interface PromptField {
  label: string;
  id: string;
  value: string;
  required?: boolean;
}

/** Small form dialog; resolves the field values, or undefined when cancelled. */
export function promptDialog(options: { title: string; submitLabel: string; fields: PromptField[]; signal?: AbortSignal }): Promise<Record<string, string> | undefined> {
  return new Promise(resolve => {
    const dialog = element("dialog", "analytics-dialog");
    dialog.setAttribute("aria-label", options.title);
    const form = element("form"), fields = options.fields.map(spec => field(spec.label, spec.id, spec.value));
    const submit = button(options.submitLabel), cancel = button(t("common.cancel"), "button secondary");
    submit.type = "submit";
    let answer: Record<string, string> | undefined;
    cancel.onclick = () => dialog.close();
    for (const [index, control] of fields.entries()) control.input.required = options.fields[index].required ?? false;
    form.onsubmit = event => {
      event.preventDefault();
      answer = Object.fromEntries(options.fields.map((spec, index) => [spec.id, fields[index].input.value]));
      dialog.close();
    };
    const actions = element("div", "dialog-actions");
    actions.append(cancel, submit);
    form.append(heading(options.title), ...fields.map(control => control.wrapper), actions);
    dialog.append(form);
    document.body.append(dialog);
    const abort = () => dialog.close();
    options.signal?.addEventListener("abort", abort, { once: true });
    dialog.addEventListener("close", () => {
      options.signal?.removeEventListener("abort", abort);
      dialog.remove();
      resolve(answer);
    }, { once: true });
    dialog.showModal();
    fields[0]?.input.focus();
  });
}

/** Page-header status: a transient success line, or a persistent error with an optional retry. */
export function statusChip(host: HTMLElement): { ok: (text: string) => void; fail: (text: string, retry?: () => void) => void; clear: () => void } {
  const chip = element("span", "status-chip");
  chip.hidden = true;
  host.append(chip);
  let timer = 0;
  const clear = () => {
    window.clearTimeout(timer);
    chip.hidden = true;
    chip.className = "status-chip";
    chip.replaceChildren();
  };
  return {
    ok: text => {
      clear();
      chip.textContent = text;
      chip.hidden = false;
      timer = window.setTimeout(clear, 1800);
    },
    fail: (text, retry) => {
      clear();
      chip.className = "status-chip error";
      chip.append(document.createTextNode(text));
      if (retry) {
        const again = element("button", "chip-action", t("common.retry"));
        again.type = "button";
        again.onclick = () => { clear(); retry(); };
        chip.append(again);
      }
      chip.hidden = false;
    },
    clear,
  };
}
export function heading(title: string, subtitle?: string): HTMLElement {
  const node = element("section", "page-heading");
  node.append(element("h1", "", title));
  if (subtitle) node.append(element("p", "subtitle", subtitle));
  return node;
}

/** Page title with its primary action on the same row, so a list never starts with a stray button. */
export function pageHeader(title: string, subtitle: string | undefined, ...actions: HTMLElement[]): HTMLElement {
  const header = element("header", "page-header");
  const bar = element("div", "page-actions");
  bar.append(...actions);
  header.append(heading(title, subtitle), bar);
  return header;
}
export function message(error: unknown): string {
  return error instanceof Error
    ? error.message
    : t("app.error.operation");
}
/** Service statuses, mapped to catalogue keys so the badge follows the active language. */
const statusLabels: Record<string, TranslationKey> = {
  pending: key("app.status.pending"),
  waiting_for_user: key("app.status.waiting_for_user"),
  saving: key("app.status.saving"),
  running: key("app.status.running"),
  completed: key("app.status.completed"),
  failed: key("app.status.failed"),
  cancelled: key("app.status.cancelled"),
  expired: key("app.status.expired"),
};
/** A status word; a status without a catalogue entry keeps its readable form. */
export function statusText(status: string): string {
  const label = statusLabels[status];
  return label ? t(label) : status.replaceAll("_", " ");
}
export function statusBadge(status: string): HTMLElement {
  return element(
    "span",
    `status status-${status}`,
    statusText(status),
  );
}
export function field(
  label: string,
  name: string,
  value: string,
  type = "text",
): { wrapper: HTMLElement; input: HTMLInputElement } {
  const wrapper = element("div", "field");
  const input = element("input");
  input.type = type;
  input.name = name;
  input.id = name;
  input.value = value;
  const caption = element("label", "", label);
  caption.htmlFor = name;
  wrapper.append(caption, input);
  return { wrapper, input };
}
export function selectField(
  label: string,
  name: string,
  choices: [string, string][],
  selected: string,
): { wrapper: HTMLElement; input: HTMLSelectElement } {
  const wrapper = element("div", "field");
  const caption = element("label", "", label);
  caption.htmlFor = name;
  const input = element("select");
  input.id = name;
  input.name = name;
  for (const [value, text] of choices) {
    const option = element("option", "", text);
    option.value = value;
    input.append(option);
  }
  input.value = selected;
  wrapper.append(caption, input);
  return { wrapper, input };
}
export async function copy(
  text: string,
  target: HTMLButtonElement,
): Promise<void> {
  const original = target.textContent;
  try {
    await navigator.clipboard.writeText(text);
    target.textContent = t("component.copied");
  } catch {
    target.textContent = t("component.copyUnavailable");
  }
  setTimeout(() => {
    target.textContent = original;
  }, 1600);
}
export function showValue(value: unknown): void {
  const dialog = element("dialog", "value-dialog");
  const title = element("h2", "", t("component.cellValue"));
  const content = element("pre", "", value === null ? t("common.null") : String(value));
  const actions = element("div", "actions");
  const copyButton = button(t("component.copyValue"));
  const close = button(t("common.close"), "button secondary");
  copyButton.onclick = () => void copy(content.textContent ?? "", copyButton);
  close.onclick = () => dialog.close();
  actions.append(copyButton, close);
  dialog.append(title, content, actions);
  document.body.append(dialog);
  dialog.addEventListener("close", () => dialog.remove());
  dialog.showModal();
}
