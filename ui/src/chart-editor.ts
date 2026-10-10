import { getCatalog, getSnapshot, loadChartData, runChart, saveChart } from "../sdk/analytics";
import type { Catalog, Chart, ChartData, ChartType, Metric } from "../sdk/analytics";
import type { Workspace } from "../sdk/types";
import { api } from "./api";
import { button, element, field, message, selectField } from "./components";
import { chartLabels, numberValue } from "./chart-option";
import { renderChart } from "./chart-view";
import { waitRun } from "./run-status";
import { t } from "./i18n";

/** Types that only ever plot one measured column. */
const singleMetric: ChartType[] = ["Statistics", "Pie", "RingPie", "RosePie", "Funnel", "WordCloud"];
/** Types that can split their rows into series. */
const seriesTypes: ChartType[] = ["Column", "Bar", "Line", "AreaLine", "Scatter", "Combo"];

/**
 * The editor is intentionally tiny: one chart, three views.
 * Views: chart · result set · SQL. Configuration: a name, a type, an X axis and a Y axis.
 * A chart owns its SQL and the datasource it runs against, so a new chart starts with an empty one.
 */
export function chartEditor(
  catalog: Catalog,
  initial: Chart | undefined,
  signal: AbortSignal,
  onSave: (chart: Chart) => Promise<void>,
  onCatalog: () => Promise<void>,
  defaults?: { datasourceId?: string },
): void {
  let chart: Chart = initial
    ? structuredClone(initial)
    : { id: crypto.randomUUID(), name: t("editor.unnamed"), datasource_id: defaults?.datasourceId ?? "", statements: [""], statement: 0, result: 0, snapshot_id: null, history: [], revision: 0, spec: { chart_type: "Column", dimension: null, group_by: [], metrics: [], stack: false, line_style: "straight" } };
  const dialog = element("dialog", "chart-editor");
  dialog.setAttribute("aria-label", t("editor.dialog"));
  const head = element("div", "editor-heading");
  const save = button(t("editor.save"));
  const cancel = button(t("common.cancel"), "button secondary");
  const headActions = element("div", "editor-heading-actions");
  headActions.append(cancel, save);
  head.append(element("h2", "", initial ? t("editor.title.edit") : t("editor.title.new")), headActions);
  const form = element("form", "editor-form"), config = element("div", "editor-config"), views = element("div", "editor-views-pane");
  const tabs = element("div", "editor-tabs");
  const canvas = element("div", "chart-preview-canvas"), sqlPane = element("div", "editor-sql-pane"), feedback = element("p", "feedback");
  views.append(tabs, canvas, sqlPane, feedback);
  form.append(config, views); dialog.append(head, form); document.body.append(dialog);
  // The header sits outside the form, so saving submits it explicitly.
  save.onclick = () => form.requestSubmit();

  // The whole configuration: a name, an X axis and a Y axis.
  const name = field(t("editor.field.name"), "chart-name", chart.name);
  let nameEdited = false;
  name.input.oninput = () => { chart.name = name.input.value; nameEdited = true; };
  const type = selectField(t("editor.field.type"), "chart-type", chartTypes(), chart.spec.chart_type);
  const datasource = selectField(t("editor.query.connection"), "chart-source", [], "");
  const dimension = selectField(t("editor.field.dimension"), "chart-dimension", [], "");
  const metricList = element("div", "metric-list");
  config.append(name.wrapper, type.wrapper, datasource.wrapper, dimension.wrapper, metricList);
  /** Single-dimension charts only; a chart saved as Combo keeps its own type in the list. */
  function chartTypes(): [string, string][] {
    const kinds: ChartType[] = ["Column", "Bar", "Line", "AreaLine", "Scatter", "Pie", "RingPie", "RosePie", "Funnel", "WordCloud", "Statistics", "Table"];
    if (!kinds.includes(chart.spec.chart_type)) kinds.push(chart.spec.chart_type);
    return kinds.map(kind => [kind, chartLabels[kind]] as [string, string]);
  }

  type View = "chart" | "result" | "sql";
  let view: View = "chart";
  const tabButtons: Record<View, HTMLButtonElement> = {
    chart: button(t("editor.view.chart"), "tab active"),
    result: button(t("editor.view.resultSet"), "tab"),
    sql: button(t("editor.view.sql"), "tab"),
  };
  for (const [kind, control] of Object.entries(tabButtons) as [View, HTMLButtonElement][]) {
    control.onclick = () => { view = kind; syncTabs(); paint(); };
    tabs.append(control);
  }
  const syncTabs = () => {
    for (const [kind, control] of Object.entries(tabButtons) as [View, HTMLButtonElement][]) control.classList.toggle("active", kind === view);
    canvas.hidden = view === "sql";
    sqlPane.hidden = view !== "sql";
  };

  const lifetime = new AbortController();
  let dispose = () => {}, data: ChartData | undefined, generation = 0, sqlDirty = false, workspace: Workspace | undefined;
  let activeSnapshot = "";
  const cache = new Map<string, Promise<ChartData>>();
  const sqlArea = element("div", "query-sql");
  const run = button(t("editor.query.run"));
  const runFeedback = element("p", "feedback query-feedback");
  const sqlInput = element("textarea", "sql-editor");
  sqlInput.id = "chart-sql";
  sqlInput.oninput = () => { chart.statements = [sqlInput.value]; sqlDirty = true; };
  const runRow = element("div", "query-run"); runRow.append(run);
  sqlArea.append(sqlInput); sqlPane.append(sqlArea, runRow, runFeedback);
  void api<Workspace>("/home", undefined, lifetime.signal).then(value => { workspace = value; renderConfig(); }).catch(() => {});
  const abort = () => { if (!sqlDirty || confirm(t("editor.discard.confirm"))) dialog.close(); };
  signal.addEventListener("abort", abort, { once: true });
  dialog.addEventListener("close", () => { lifetime.abort(); dispose(); signal.removeEventListener("abort", abort); dialog.remove(); });
  cancel.onclick = () => abort();

  const columns = () => data?.columns ?? [];
  const numeric = (column: string) => data?.rows.some(row => { try { return numberValue(row[data!.columns.indexOf(column)]) !== null; } catch { return false; } }) ?? false;
  /** A chart nobody has run yet: it needs a connection and some SQL. */
  const isDraft = () => !chart.snapshot_id && !chart.statements.join("").trim();

  /** Chart, result set or SQL: one area, three readings of the same snapshot. */
  function paint() {
    dispose(); dispose = () => {};
    syncTabs();
    if (view === "sql") { feedback.className = "feedback"; feedback.textContent = ""; return; }
    if (!data) {
      canvas.replaceChildren();
      const empty = element("div", "empty-state");
      empty.append(element("p", "muted", t("editor.query.empty")));
      // Says what to do next rather than naming internals: write SQL, run it, the chart appears.
      const go = button(t("editor.query.writeSql"));
      go.onclick = () => { view = "sql"; syncTabs(); paint(); };
      empty.append(go);
      canvas.append(empty);
      return;
    }
    feedback.className = "feedback"; feedback.textContent = "";
    try {
      // The result view describes the snapshot itself, so it ignores bindings and their validity.
      const spec = view === "result" ? { chart_type: "Table" as const, dimension: null, group_by: [], metrics: [], stack: false, line_style: "straight" as const } : chart.spec;
      dispose = renderChart(canvas, spec, data);
    } catch (error) {
      canvas.replaceChildren();
      feedback.className = "feedback error"; feedback.textContent = message(error);
    }
  }

  /** The whole configuration: the X column, the Y column and the connection for a brand-new chart. */
  function renderConfig() {
    const available = columns();
    // A saved chart may still group its rows into series; those columns stay out of the Y list.
    const grouped = new Set(chart.spec.group_by);
    const bindable = available.filter(column => !grouped.has(column) && column !== chart.spec.dimension);
    // Bindings only appear where the chart type actually uses them: a table lists every column,
    // a statistic card has no axis, and the rest need an X and a Y.
    const usesDimension = chart.spec.chart_type !== "Table" && chart.spec.chart_type !== "Statistics";
    const usesMetric = chart.spec.chart_type !== "Table";
    // A brand-new chart takes the first connection; the choice only shows up when there is one to make.
    const sources = workspace?.datasources ?? [];
    if (!chart.datasource_id && sources.length) chart.datasource_id = sources[0].id;
    datasource.wrapper.hidden = !isDraft() || sources.length < 2;
    if (!datasource.wrapper.hidden) {
      datasource.input.replaceChildren(...sources.map(source => { const option = element("option", "", source.name); option.value = source.id; return option; }));
      datasource.input.value = chart.datasource_id;
    }
    dimension.wrapper.hidden = !usesDimension;
    if (usesDimension) {
      dimension.input.replaceChildren(element("option", "", t("common.none")), ...available.map(column => { const option = element("option", "", column); option.value = column; return option; }));
      dimension.input.value = chart.spec.dimension ?? "";
    }
    // Exactly one Y axis. A chart saved with extra metrics shows the first one; picking a field
    // replaces the whole list, so the simplified editor never grows a second series by accident.
    const metric = chart.spec.metrics[0];
    metricList.replaceChildren();
    metricList.hidden = !usesMetric;
    if (metric && usesMetric) {
      const row = element("div", "metric-row");
      // Y only offers numbers (plus whatever the chart already uses), so a text column cannot be picked by mistake.
      const numericOnly = available.filter(column => numeric(column));
      const choices = [...new Set([metric.field, ...(numericOnly.length ? numericOnly : bindable)])];
      const select = selectField(t("editor.field.metric"), "metric-0", choices.map(column => [column, column]), metric.field);
      select.input.onchange = () => { chart.spec.metrics = [{ ...metric, field: select.input.value }]; renderConfig(); paint(); };
      row.append(select.wrapper);
      metricList.append(row);
    }
    if (usesMetric && !available.length) metricList.append(element("p", "muted", t("editor.metric.need")));
    sqlInput.value = chart.statements[0] ?? "";
  }

  run.onclick = () => void runQuery();
  async function runQuery(): Promise<void> {
    runFeedback.className = "feedback query-feedback"; runFeedback.textContent = t("editor.query.running");
    generation++;
    try {
      // Save first: the run publishes against the stored SQL, then the catalog is re-read.
      chart = await saveChart(chart);
      sqlDirty = false;
      await waitRun(await runChart(chart.id), lifetime.signal);
      await onCatalog();
      const fresh = await getCatalog(lifetime.signal);
      chart = fresh.charts.find(c => c.id === chart.id) ?? chart;
      cache.clear();
      runFeedback.className = "feedback query-feedback"; runFeedback.textContent = t("editor.query.done");
      renderConfig();
      await load(true);
    } catch (error) { if (!lifetime.signal.aborted) { runFeedback.className = "feedback error query-feedback"; runFeedback.textContent = message(error); } }
  }

  async function load(reset: boolean): Promise<void> {
    const ticket = ++generation; save.disabled = true;
    const snapshot = chart.snapshot_id ?? undefined;
    if (!snapshot) {
      data = undefined; activeSnapshot = "";
      renderConfig(); paint(); save.disabled = false; return;
    }
    feedback.className = "feedback"; feedback.textContent = t("editor.loading");
    try {
      activeSnapshot = snapshot;
      const meta = await getSnapshot(activeSnapshot, lifetime.signal);
      if (ticket !== generation) return;
      const tables = meta.tables.filter(table => table.columns.length);
      if (!tables.length) throw new Error(t("editor.query.empty"));
      if (!tables.some(table => table.statement === chart.statement && table.result === chart.result)) { chart.statement = tables[0].statement; chart.result = tables[0].result; }
      const key = `${activeSnapshot}:${chart.statement}:${chart.result}`;
      if (!cache.has(key)) cache.set(key, loadChartData(activeSnapshot, chart.statement, chart.result, lifetime.signal));
      const loaded = await cache.get(key)!;
      if (ticket !== generation || lifetime.signal.aborted) return;
      data = loaded;
      if (reset || !chart.spec.metrics.length) seedBindings();
      renderConfig(); paint();
    } catch (error) {
      if (!lifetime.signal.aborted) { data = undefined; feedback.className = "feedback error"; feedback.textContent = message(error); renderConfig(); paint(); }
    } finally { if (ticket === generation) save.disabled = false; }
  }
  /** A first guess that makes a new chart show something immediately. */
  function seedBindings() {
    const available = columns();
    const isStatistic = chart.spec.chart_type === "Statistics";
    chart.spec.dimension = isStatistic ? null : available[0] ?? null;
    chart.spec.group_by = [];
    const numbers = available.filter(column => numeric(column));
    const metric = numbers.find(column => column !== chart.spec.dimension) ?? numbers[0];
    chart.spec.metrics = metric ? [{ field: metric, label: "", unit: "", axis: "left", kind: chart.spec.chart_type === "Combo" ? "Column" : null }] : [];
  }
  type.input.onchange = () => {
    chart.spec.chart_type = type.input.value as ChartType;
    if (singleMetric.includes(chart.spec.chart_type)) chart.spec.metrics = chart.spec.metrics.slice(0, 1);
    if (!seriesTypes.includes(chart.spec.chart_type)) { chart.spec.group_by = []; chart.spec.stack = false; }
    if (chart.spec.chart_type === "Statistics") chart.spec.dimension = null;
    else if (!chart.spec.dimension) chart.spec.dimension = columns()[0] ?? null;
    for (const metric of chart.spec.metrics as Metric[]) metric.kind = chart.spec.chart_type === "Combo" ? metric.kind ?? "Column" : null;
    renderConfig(); paint();
  };
  datasource.input.onchange = () => { chart.datasource_id = datasource.input.value; sqlDirty = true; };
  dimension.input.onchange = () => { chart.spec.dimension = dimension.input.value || null; renderConfig(); paint(); };
  form.onsubmit = async event => {
    event.preventDefault(); if (save.disabled) return; save.disabled = true;
    try {
      if (!chart.statements.join("").trim()) {
        // Saving without SQL would leave the chart pointing at nothing.
        throw new Error(t("editor.query.needSql"));
      }
      // A brand-new chart that was never renamed is titled after what it plots.
      const derived = [chart.spec.metrics[0]?.field, chart.spec.dimension].filter(Boolean).join(" · ");
      if (!chart.name.trim() || (!nameEdited && !initial && derived)) chart.name = derived || t("editor.unnamed");
      name.input.value = chart.name;
      chart = await saveChart(chart); await onSave(chart); dialog.close();
    } catch (error) { feedback.className = "feedback error"; feedback.textContent = message(error); }
    finally { save.disabled = false; }
  };

  syncTabs();
  dialog.showModal();
  void load(false);
}
