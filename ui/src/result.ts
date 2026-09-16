import { api } from "./api";
import { pollLater } from "./navigation";
import { refreshControls } from "./refresh";
import {
  button,
  copy,
  element,
  heading,
  message,
  showValue,
  resultIcon,
} from "./components";

import type {
  ResultTable as Table,
  ResultMetadata as Metadata,
  ResultPage as Page,
} from "../sdk/types";

const running = (status: string) => status === "running" || status === "queued";

export async function resultPage(
  root: HTMLElement,
  id: string,
  onStatusChange: (status: string) => void,
  signal: AbortSignal,
): Promise<void> {
  let meta = await api<Metadata>(`/results/${id}`, undefined, signal);
  signal.throwIfAborted();
  let selected = 0;
  let offset = 0;
  let history: number[] = [];
  let nextOffset = 0;
  let pageSerial = 0;
  let pageLimit = 100;
  let lastState = "";
  root.className = "results-page";
  const pageHeading = heading(meta.datasource_name, "Query results");
  root.replaceChildren(pageHeading);
  const refreshUi = refreshControls(id, signal, refresh);
  refreshUi.update(meta);
  const cancel = button("Cancel query", "button secondary");
  cancel.hidden = !running(meta.status) && meta.refresh?.status !== "running";
  const sqlDetails = element("details", "sql-details");
  sqlDetails.append(element("summary", "", "SQL"));
  const sql = element(
    "pre",
    "",
    meta.statements
      .map((s, i) => "-- Statement " + (i + 1) + "\n" + s)
      .join("\n\n"),
  );
  const copySql = button("Copy SQL", "button text-button");
  copySql.onclick = () => void copy(meta.statements.join("\n"), copySql);
  sqlDetails.append(copySql, sql);
  const errors = element("div", "result-errors");
  const card = element("section", "card result-card");
  const tabs = element("div", "tabs");
  tabs.setAttribute("role", "tablist");
  tabs.setAttribute("aria-label", "Result sets");
  tabs.hidden = meta.tables.length <= 1;
  const viewport = element("div", "table-viewport");
  viewport.tabIndex = 0;
  viewport.id = "result-table";
  viewport.setAttribute("role", "tabpanel");
  viewport.setAttribute("aria-label", "Query result table");
  const count = element("span", "muted");
  const controls = element("div", "page-controls");
  const size = element("select");
  size.setAttribute("aria-label", "Rows per page");
  for (const value of [25, 50, 100, 200]) {
    const o = element("option", "", `${value} rows`);
    o.value = String(value);
    size.append(o);
  }
  size.value = "100";
  const previous = resultIcon("Previous page", "m14 6-6 6 6 6");
  const next = resultIcon("Next page", "m10 6 6 6-6 6");
  controls.append(previous, next, size);
  const commandBar = element("div", "result-command-bar");
  const footer = element("div", "result-status-bar");
  commandBar.append(cancel, refreshUi.controls);
  footer.append(count, controls);
  card.append(tabs, commandBar, viewport, footer);
  root.append(sqlDetails, refreshUi.feedback, errors, card);
  function messages() {
    errors.replaceChildren();
    for (const event of meta.events) {
      if (event.event === "error") {
        const node = element(
          "div",
          "feedback error",
          `${event.index === undefined || event.index === null ? "Execution" : `Statement ${event.index + 1}`}: ${event.message ?? "Execution failed"}${event.outcome === "unknown" ? " The database outcome is unknown; do not retry automatically." : ""}`,
        );
        errors.append(node);
      }
      if (event.event === "skipped")
        errors.append(
          element(
            "p",
            "muted",
            `Statement ${(event.index ?? 0) + 1} was not executed.`,
          ),
        );
    }
  }
  function renderTabs() {
    tabs.hidden = meta.tables.length <= 1;
    tabs.replaceChildren();
    if (tabs.hidden) return;
    meta.tables.forEach((table, i) => {
      const tab = button(
        `Statement ${table.statement + 1} · Result ${table.result + 1}`,
        `tab${i === selected ? " selected" : ""}`,
      );
      tab.setAttribute("role", "tab");
      tab.setAttribute("aria-selected", String(i === selected));
      tab.tabIndex = i === selected ? 0 : -1;
      tab.id = `result-tab-${i}`;
      tab.setAttribute("aria-controls", "result-table");
      const select = (index: number) => {
        selected = index;
        offset = 0;
        history = [];
        renderTabs();
        void loadPage();
        (tabs.children[index] as HTMLButtonElement).focus();
      };
      tab.onclick = () => select(i);
      tab.onkeydown = (event) => {
        const count = meta.tables.length;
        const index =
          event.key === "ArrowRight"
            ? (i + 1) % count
            : event.key === "ArrowLeft"
              ? (i + count - 1) % count
              : event.key === "Home"
                ? 0
                : event.key === "End"
                  ? count - 1
                  : undefined;
        if (index !== undefined) {
          event.preventDefault();
          select(index);
        }
      };
      tabs.append(tab);
    });
  }
  function tableNode(table: Table, rows: unknown[][]): HTMLElement {
    const node = element("table", "data-table");
    const head = element("thead");
    const titles = element("tr");
    for (const column of table.columns) {
      const th = element("th");
      th.scope = "col";
      th.append(
        element("span", "", column.name),
        element(
          "small",
          "",
          column.database_type +
            (column.encoding === "base64" ? " · Base64" : ""),
        ),
      );
      titles.append(th);
    }
    head.append(titles);
    node.append(head);
    const numberHeading = element("th", "row-number", "#");
    numberHeading.scope = "col";
    numberHeading.setAttribute("aria-label", "Row number");
    titles.prepend(numberHeading);
    const body = element("tbody");
    for (const [index, row] of rows.entries()) {
      const tr = element("tr");
      const rowNumber = element("th", "row-number", String(offset + index + 1));
      rowNumber.scope = "row";
      tr.append(rowNumber);
      for (const value of row) {
        const td = element("td");
        if (value === null) td.append(element("span", "null-value", "NULL"));
        else if (value === "") {
          const empty = element("span", "empty-value", "empty");
          empty.setAttribute("aria-label", "Empty string");
          td.append(empty);
        } else {
          const text = String(value);
          const cell = button(
            text.length > 160 ? text.slice(0, 160) + "…" : text,
            "cell-value",
          );
          cell.title = "Open or copy the full value";
          cell.onclick = () => showValue(value);
          td.append(cell);
        }
        tr.append(td);
      }
      body.append(tr);
    }
    node.append(body);
    return node;
  }
  async function loadPage() {
    if (signal.aborted) return;
    const serial = ++pageSerial;
    const table = meta.tables[selected];
    if (table && meta.tables.length > 1)
      viewport.setAttribute("aria-labelledby", `result-tab-${selected}`);
    else viewport.removeAttribute("aria-labelledby");
    previous.disabled = !history.length;
    next.disabled = true;
    if (!table) {
      viewport.replaceChildren(
        element(
          "p",
          "empty-state",
          running(meta.status)
            ? "Waiting for the first result…"
            : "This execution produced no result sets.",
        ),
      );
      count.textContent = "";
      return;
    }
    if (!table.columns.length) {
      viewport.replaceChildren(
        element(
          "div",
          "empty-state",
          table.affected_rows === null
            ? "Statement completed."
            : `${table.affected_rows} row(s) affected.`,
        ),
      );
      count.textContent = "";
      return;
    }
    try {
      const page = await api<Page>(
        `/results/${id}/rows?statement=${table.statement}&result=${table.result}&offset=${offset}&limit=${pageLimit}&snapshot=${encodeURIComponent(meta.snapshot ?? "initial")}`,
        undefined,
        signal,
      );
      if (signal.aborted || serial !== pageSerial) return;
      viewport.replaceChildren(
        page.rows.length
          ? tableNode(table, page.rows)
          : element(
              "p",
              "empty-state",
              table.complete ? "No rows returned." : "Loading rows…",
            ),
      );
      nextOffset = page.next_offset;
      next.disabled = nextOffset >= page.total_rows;
      previous.disabled = !history.length;
      count.textContent = `${page.rows.length ? offset + 1 : 0}–${nextOffset} of ${page.total_rows.toLocaleString()} rows${page.complete ? "" : " · loading"}`;
    } catch (error) {
      if (!signal.aborted && serial === pageSerial)
        viewport.replaceChildren(
          element("p", "feedback error", message(error)),
        );
    }
  }
  previous.onclick = () => {
    offset = history.pop() ?? 0;
    void loadPage();
  };
  next.onclick = () => {
    history.push(offset);
    offset = nextOffset;
    void loadPage();
  };
  size.onchange = () => {
    pageLimit = Number(size.value);
    offset = 0;
    history = [];
    void loadPage();
  };
  cancel.onclick = async () => {
    if (signal.aborted) return;
    cancel.disabled = true;
    try {
      await api(`/results/${id}/cancel`, {});
      cancel.textContent = "Cancelling…";
    } catch (error) {
      if (signal.aborted) return;
      errors.append(element("p", "feedback error", message(error)));
    }
  };
  async function refresh() {
    if (signal.aborted) return;
    try {
      const previousSnapshot = meta.snapshot;
      meta = await api<Metadata>(`/results/${id}`, undefined, signal);
      signal.throwIfAborted();
      if (previousSnapshot !== meta.snapshot) {
        selected = Math.max(0, Math.min(selected, meta.tables.length - 1));
        offset = 0;
        history = [];
        ++pageSerial;
      }
      const refreshing = meta.refresh?.status === "running";
      onStatusChange(meta.status);
      pageHeading.querySelector("h1")!.textContent = meta.datasource_name;
      cancel.hidden = !running(meta.status) && !refreshing;
      cancel.disabled = false;
      cancel.textContent = refreshing ? "Cancel refresh" : "Cancel query";
      refreshUi.update(meta);
      const state = JSON.stringify([meta.snapshot, meta.status, meta.tables]);
      if (state !== lastState) {
        lastState = state;
        renderTabs();
        messages();
        await loadPage();
      }
      if (running(meta.status) || refreshing) pollLater(signal, refresh);
    } catch (error) {
      if (signal.aborted) return;
      refreshUi.stop(message(error));
      errors.replaceChildren(element("p", "feedback error", message(error)));
    }
  }
  await refresh();
  window.addEventListener(
    "focus",
    () => {
      if (!running(meta.status) && meta.refresh?.status !== "running")
        void refresh();
    },
    { signal },
  );
}
