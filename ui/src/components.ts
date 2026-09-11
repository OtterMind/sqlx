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
export function heading(
  eyebrow: string,
  title: string,
  subtitle: string,
): HTMLElement {
  const node = element("section", "page-heading");
  node.append(
    element("p", "eyebrow", eyebrow),
    element("h1", "", title),
    element("p", "subtitle", subtitle),
  );
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
