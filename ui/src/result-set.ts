import { button, element, message, showValue } from "./components";
import { t, tn } from "./i18n";

export interface ResultSetColumn {
  name: string;
  /** Optional second line under the header, e.g. the database type. */
  detail?: string;
}

export interface ResultSetPage {
  rows: unknown[][];
  next_offset: number;
  total_rows: number;
  complete: boolean;
}

export interface ResultSetOptions {
  columns: ResultSetColumn[];
  /** Client-side rows; omit when the page contents come from loadPage. */
  rows?: unknown[][];
  /** Total row count when it is known before the first page loads (server-side paging). */
  total?: number;
  /** Server-side paging. Called for every page change. */
  loadPage?: (offset: number, limit: number) => Promise<ResultSetPage>;
  /** Rows per page choices; the same list everywhere unless a caller really needs another. */
  pageSizes?: number[];
  /** Initially selected page size; defaults to 100 when it is offered. */
  pageSize?: number;
  /** Optional ceiling for a custom page size; by default any size is accepted. */
  maxPageSize?: number;
  showRowNumbers?: boolean;
  label?: string;
  signal?: AbortSignal;
}

/**
 * The one result-set table in the product: sticky header, exact cell values (click to inspect),
 * row numbers, numbered paging with first/last, a page-size selector and a range readout.
 * Used by the result page, the chart editor's result view and Table charts.
 */
export function resultSet(options: ResultSetOptions): { element: HTMLElement; reload: (reset?: boolean) => Promise<void>; goToPage: (page: number) => Promise<void> } {
  const pageSizes = options.pageSizes ?? [25, 50, 100, 200];
  const ceiling = options.maxPageSize ?? Number.MAX_SAFE_INTEGER;
  const initial = options.pageSize ?? (pageSizes.includes(100) ? 100 : pageSizes[0]);
  let limit = initial, offset = 0, total = options.rows?.length ?? options.total ?? 0, complete = true;
  let rows: unknown[][] = [];
  let serial = 0;
  const root = element("div", "result-set");
  const viewport = element("div", "table-viewport analytics-table-scroll");
  viewport.tabIndex = 0;
  if (options.label) viewport.setAttribute("aria-label", options.label);
  const table = element("table", "data-table analytics-table");
  const head = element("thead"), titles = element("tr");
  if (options.showRowNumbers !== false) {
    const number = element("th", "row-number", "#");
    number.scope = "col";
    number.setAttribute("aria-label", t("result.rowNumber"));
    titles.append(number);
  }
  for (const column of options.columns) {
    const cell = element("th"); cell.scope = "col";
    cell.append(element("span", "", column.name));
    if (column.detail) cell.append(element("small", "", column.detail));
    titles.append(cell);
  }
  head.append(titles);
  const body = element("tbody");
  table.append(head, body);
  viewport.append(table);
  const controls = element("div", "table-controls");
  const pager = element("div", "table-pager");
  const first = button("«", "button secondary"), previous = button(t("chart.view.previous"), "button secondary"), numbers = element("div", "page-numbers"), next = button(t("chart.view.next"), "button secondary"), last = button("»", "button secondary");
  first.title = t("chart.view.first"); last.title = t("chart.view.last");
  const range = element("span", "muted");
  // One dropdown: the presets, with a custom size typed straight into it.
  const sizeLabel = element("div", "table-page-size");
  const picker = element("div", "size-picker");
  const trigger = button(t("result.rowsOption", { count: initial }), "size-trigger");
  trigger.type = "button";
  trigger.setAttribute("aria-haspopup", "listbox");
  trigger.setAttribute("aria-expanded", "false");
  trigger.title = t("result.rowsPerPage");
  const panel = element("div", "size-panel");
  panel.setAttribute("role", "listbox");
  panel.setAttribute("aria-label", t("result.rowsPerPage"));
  panel.hidden = true;
  const optionList = element("div", "size-options");
  const items = new Map<number, HTMLButtonElement>();
  for (const value of pageSizes) {
    const item = button(t("result.rowsOption", { count: value }), "size-option");
    item.type = "button";
    item.setAttribute("role", "option");
    item.onclick = () => { applySize(value); closePanel(); };
    items.set(value, item);
    optionList.append(item);
  }
  const customRow = element("div", "size-custom-row");
  const custom = element("input", "size-custom");
  custom.type = "number"; custom.min = "1"; custom.step = "1";
  custom.placeholder = t("result.rowsCustom");
  custom.setAttribute("aria-label", t("result.rowsCustom"));
  const customTick = element("span", "size-tick", "✓");
  customRow.append(custom, customTick);
  panel.append(optionList, element("div", "size-separator"), customRow);
  picker.append(trigger, panel);
  sizeLabel.append(document.createTextNode(t("result.rowsPerPage")), picker);
  const closePanel = () => { panel.hidden = true; trigger.setAttribute("aria-expanded", "false"); };
  const openPanel = () => {
    panel.hidden = false;
    trigger.setAttribute("aria-expanded", "true");
    custom.value = pageSizes.includes(limit) ? "" : String(limit);
  };
  trigger.onclick = () => { if (panel.hidden) openPanel(); else closePanel(); };
  const outside = (event: MouseEvent) => { if (!picker.contains(event.target as Node)) closePanel(); };
  const escape = (event: KeyboardEvent) => { if (event.key === "Escape" && !panel.hidden) { closePanel(); trigger.focus(); } };
  document.addEventListener("click", outside);
  document.addEventListener("keydown", escape);
  optionList.addEventListener("keydown", (event: KeyboardEvent) => { if (event.key === "Escape") { closePanel(); trigger.focus(); } });
  controls.append(pager, sizeLabel, range);
  const feedback = element("p", "feedback result-set-feedback");
  root.append(viewport, controls, feedback);

  const pages = () => Math.max(1, Math.ceil(total / limit));
  /** A compact window of page numbers with ellipses, so long results stay navigable. */
  function pageWindow(current: number, count: number): (number | "gap")[] {
    if (count <= 7) return Array.from({ length: count }, (_, index) => index + 1);
    const marks = new Set<number>([1, count, current, current - 1, current + 1]);
    const sorted = [...marks].filter(page => page >= 1 && page <= count).sort((a, b) => a - b);
    const out: (number | "gap")[] = [];
    for (const [index, page] of sorted.entries()) {
      if (index && page - sorted[index - 1] > 1) out.push("gap");
      out.push(page);
    }
    return out;
  }
  function cellFor(value: unknown): HTMLElement {
    const td = element("td");
    if (value === null) { td.append(element("span", "null-value", t("result.null"))); return td; }
    if (value === "") { const empty = element("span", "empty-value", t("component.emptyValue")); empty.setAttribute("aria-label", t("result.empty")); td.append(empty); return td; }
    const text = String(value);
    const cell = button(text.length > 160 ? text.slice(0, 160) + "…" : text, "cell-value");
    cell.title = t("result.openValue");
    cell.onclick = () => showValue(value);
    td.append(cell);
    return td;
  }
  function paint() {
    const current = Math.floor(offset / limit) + 1;
    body.replaceChildren(...rows.map((row, index) => {
      const tr = element("tr");
      if (options.showRowNumbers !== false) { const number = element("th", "row-number", String(offset + index + 1)); number.scope = "row"; tr.append(number); }
      for (const value of row) tr.append(cellFor(value));
      return tr;
    }));
    numbers.replaceChildren(...pageWindow(current, pages()).map(entry => entry === "gap"
      ? element("span", "page-gap", "…")
      : (() => {
          const control = element("button", current === entry ? "page-number active" : "page-number", String(entry));
          control.type = "button";
          control.setAttribute("aria-label", t("chart.view.page", { page: entry }));
          if (current === entry) control.setAttribute("aria-current", "page");
          control.onclick = () => void goTo(entry);
          return control;
        })()));
    first.disabled = previous.disabled = current === 1;
    next.disabled = last.disabled = current === pages();
    const from = rows.length ? offset + 1 : 0;
    const to = Math.min(offset + limit, total);
    range.textContent = complete
      ? tn("chart.view.range.one", "chart.view.range", total, { from, to, total })
      : t("result.rangeLoading", { from, to, total: total.toLocaleString() });
    range.title = t("chart.view.pages", { pages: pages() });
  }
  async function fetchPage(): Promise<void> {
    if (!options.loadPage) {
      rows = (options.rows ?? []).slice(offset, offset + limit);
      total = options.rows?.length ?? 0;
      complete = true;
      paint();
      return;
    }
    const ticket = ++serial;
    try {
      const page = await options.loadPage(offset, limit);
      if (ticket !== serial || options.signal?.aborted) return;
      rows = page.rows; total = page.total_rows; complete = page.complete;
      feedback.className = "feedback result-set-feedback";
      feedback.textContent = "";
      paint();
    } catch (error) {
      if (ticket !== serial || options.signal?.aborted) return;
      // The table stays as it was, so a rejected page size or a transient error is recoverable.
      feedback.className = "feedback error result-set-feedback";
      feedback.textContent = message(error);
      paint();
    }
  }
  async function goTo(page: number): Promise<void> {
    const target = Math.min(Math.max(1, page), pages());
    if (target === Math.floor(offset / limit) + 1) return;
    offset = (target - 1) * limit;
    await fetchPage();
  }
  first.onclick = () => void goTo(1);
  previous.onclick = () => void goTo(Math.floor(offset / limit));
  next.onclick = () => void goTo(Math.floor(offset / limit) + 2);
  last.onclick = () => void goTo(pages());
  const applySize = (value: number) => {
    limit = Math.min(ceiling, Math.max(1, Math.floor(value) || limit));
    offset = 0;
    // The trigger always names the size in use, and exactly one row carries the selected state:
    // its preset, or the custom field when the size is not one of the presets.
    trigger.textContent = t("result.rowsOption", { count: limit });
    const preset = pageSizes.includes(limit);
    for (const [value_, item] of items) {
      const selected = preset && value_ === limit;
      item.classList.toggle("active", selected);
      item.setAttribute("aria-selected", String(selected));
    }
    custom.classList.toggle("active", !preset);
    customRow.classList.toggle("active", !preset);
    void fetchPage();
  };
  custom.onkeydown = event => {
    if (event.key !== "Enter") return;
    event.preventDefault();
    applySize(Number(custom.value));
    closePanel();
  };
  applySize(initial);
  pager.append(first, previous, numbers, next, last);
  void fetchPage();
  return {
    element: root,
    reload: async (reset = false) => { if (reset) offset = 0; await fetchPage(); },
    goToPage: goTo,
  };
}
