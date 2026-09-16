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
export function heading(title: string, subtitle?: string): HTMLElement {
  const node = element("section", "page-heading");
  node.append(element("h1", "", title));
  if (subtitle) node.append(element("p", "subtitle", subtitle));
  return node;
}
export function message(error: unknown): string {
  return error instanceof Error
    ? error.message
    : "The operation could not be completed.";
}
export function statusBadge(status: string): HTMLElement {
  return element(
    "span",
    `status status-${status}`,
    status.replaceAll("_", " "),
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
    target.textContent = "Copied";
  } catch {
    target.textContent = "Copy unavailable";
  }
  setTimeout(() => {
    target.textContent = original;
  }, 1600);
}
export function showValue(value: unknown): void {
  const dialog = element("dialog", "value-dialog");
  const title = element("h2", "", "Cell value");
  const content = element("pre", "", value === null ? "NULL" : String(value));
  const actions = element("div", "actions");
  const copyButton = button("Copy value");
  const close = button("Close", "button secondary");
  copyButton.onclick = () => void copy(content.textContent ?? "", copyButton);
  close.onclick = () => dialog.close();
  actions.append(copyButton, close);
  dialog.append(title, content, actions);
  document.body.append(dialog);
  dialog.addEventListener("close", () => dialog.remove());
  dialog.showModal();
}
