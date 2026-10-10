import { api } from "./api";
import { pollLater } from "./navigation";
import { refreshControls } from "./refresh";
import { button, copy, element, heading, message } from "./components";
import { resultSet } from "./result-set";
import { t } from "./i18n";

import type {
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
  let lastState = "";
  root.className = "results-page";
  // The label the query was submitted with wins over the datasource name.
  const pageHeading = heading(meta.description?.trim() || meta.datasource_name, t("result.heading"));
  root.replaceChildren(pageHeading);
  const refreshUi = refreshControls(id, signal, refresh);
  refreshUi.update(meta);
  const cancel = button(t("result.cancel"), "button secondary");
  cancel.hidden = !running(meta.status) && meta.refresh?.status !== "running";
  const sqlDetails = element("details", "sql-details");
  sqlDetails.append(element("summary", "", t("result.column.sql")));
  const sql = element("pre", "", meta.statements.map((statement, index) => "-- Statement " + (index + 1) + "\n" + statement).join("\n\n"));
  const copySql = button(t("result.copySql"), "button text-button");
  copySql.onclick = () => void copy(meta.statements.join("\n"), copySql);
  sqlDetails.append(copySql, sql);
  const errors = element("div", "result-errors");
  const card = element("section", "card result-card");
  const tabs = element("div", "tabs");
  tabs.setAttribute("role", "tablist");
  tabs.setAttribute("aria-label", t("result.resultSets"));
  tabs.hidden = meta.tables.length <= 1;
  const tableArea = element("div", "result-table-area");
  const commandBar = element("div", "result-command-bar");
  commandBar.append(cancel, refreshUi.controls);
  card.append(tabs, commandBar, tableArea);
  root.append(sqlDetails, refreshUi.feedback, errors, card);

  function messages() {
    errors.replaceChildren();
    for (const event of meta.events) {
      if (event.event === "error") {
        errors.append(element("div", "feedback error", `${t("result.error.statement", { statement: (event.index ?? 0) + 1, error: event.message ?? "" })}${event.outcome === "unknown" ? " " + t("result.error.unknownOutcome") : ""}`));
      }
      if (event.event === "skipped")
        errors.append(element("p", "muted", t("result.statementSkipped", { statement: (event.index ?? 0) + 1 })));
    }
  }
  const selectedTable = () => meta.tables[selected];

  /** One result set at a time, always through the shared table component. */
  function renderTable() {
    const table = selectedTable();
    tableArea.replaceChildren();
    if (!table) {
      tableArea.append(element("p", "empty-state", running(meta.status) ? t("result.waiting") : t("result.noResultSets")));
      return;
    }
    if (!table.columns.length) {
      tableArea.append(element("div", "empty-state", table.affected_rows === null ? t("result.statementDone") : t("result.rowsAffected", { count: table.affected_rows })));
      return;
    }
    const current = table;
    const view = resultSet({
      columns: current.columns.map(column => ({ name: column.name, detail: column.database_type + (column.encoding === "base64" ? " · " + t("result.base64") : "") })),
      label: t("result.table"),
      signal,
      loadPage: async (offset, limit): Promise<Page> => api<Page>(
        `/results/${id}/rows?statement=${current.statement}&result=${current.result}&offset=${offset}&limit=${limit}&snapshot=${encodeURIComponent(meta.snapshot ?? "initial")}`,
        undefined,
        signal,
      ),
    });
    if (meta.tables.length > 1) view.element.setAttribute("aria-labelledby", `result-tab-${selected}`);
    tableArea.append(view.element);
  }

  function renderTabs() {
    tabs.hidden = meta.tables.length <= 1;
    tabs.replaceChildren();
    if (tabs.hidden) return;
    meta.tables.forEach((table, index) => {
      const tab = button(t("result.statementTab", { statement: table.statement + 1, result: table.result + 1 }), `tab${index === selected ? " selected" : ""}`);
      tab.setAttribute("role", "tab");
      tab.setAttribute("aria-selected", String(index === selected));
      tab.tabIndex = index === selected ? 0 : -1;
      tab.id = `result-tab-${index}`;
      const select = (next: number) => {
        selected = next;
        renderTabs();
        renderTable();
        (tabs.children[next] as HTMLButtonElement).focus();
      };
      tab.onclick = () => select(index);
      tab.onkeydown = event => {
        const count = meta.tables.length;
        const next = event.key === "ArrowRight" ? (index + 1) % count
          : event.key === "ArrowLeft" ? (index + count - 1) % count
          : event.key === "Home" ? 0
          : event.key === "End" ? count - 1
          : undefined;
        if (next !== undefined) { event.preventDefault(); select(next); }
      };
      tabs.append(tab);
    });
  }

  cancel.onclick = async () => {
    if (signal.aborted) return;
    cancel.disabled = true;
    try {
      await api(`/results/${id}/cancel`, {});
      cancel.textContent = t("result.cancelling");
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
      const changed = previousSnapshot !== meta.snapshot;
      if (changed) selected = Math.max(0, Math.min(selected, meta.tables.length - 1));
      const refreshing = meta.refresh?.status === "running";
      onStatusChange(meta.status);
      pageHeading.querySelector("h1")!.textContent = meta.datasource_name;
      cancel.hidden = !running(meta.status) && !refreshing;
      cancel.disabled = false;
      cancel.textContent = refreshing ? t("result.cancelRefresh") : t("result.cancel");
      refreshUi.update(meta);
      const state = JSON.stringify([meta.snapshot, meta.status, meta.tables]);
      if (state !== lastState) {
        lastState = state;
        renderTabs();
        renderTable();
        messages();
      }
      if (running(meta.status) || refreshing) pollLater(signal, refresh);
    } catch (error) {
      if (signal.aborted) return;
      refreshUi.stop(message(error));
      errors.replaceChildren(element("p", "feedback error", message(error)));
    }
  }
  renderTabs();
  renderTable();
  messages();
  if (running(meta.status) || meta.refresh?.status === "running") pollLater(signal, refresh);
}
