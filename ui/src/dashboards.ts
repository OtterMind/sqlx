import { dashboardSettings } from "./dashboard-settings";
import { GridStack, type GridStackWidget } from "gridstack";
import { deleteObject, getCatalog, loadChartData, runChart, saveDashboard } from "../sdk/analytics";
import type { Catalog, Chart, ChartData, Dashboard } from "../sdk/analytics";
import { button, cardActions, cardIcons, confirmDialog, element, heading, iconLabelButton, message, overflowMenu, pageHeader, promptDialog, resultIcon, statusChip, svgIcon } from "./components";
import { chartLabels } from "./chart-option";
import { chartEditor } from "./chart-editor";
import { renderChart } from "./chart-view";
import { navigate } from "./navigation";
import { waitRun } from "./run-status";
import { t, tn } from "./i18n";

/**
 * Deleting a dashboard takes its contents with it: every chart placed on it, and every query those
 * charts owned once nothing else refers to them. Charts or queries still used elsewhere survive.
 */
async function deleteBoardWithContents(board: Dashboard, signal: AbortSignal): Promise<string[]> {
  const chartIds = board.charts.map(placement => placement.chart_id);
  const failures: string[] = [];
  await deleteObject("dashboard", board.id, board.revision);
  let catalog = await getCatalog(signal);
  for (const chartId of chartIds) {
    if (signal.aborted) break;
    const chart = catalog.charts.find(c => c.id === chartId);
    if (!chart) continue;
    try {
      // Charts belong to one board; a leftover placement elsewhere is removed before the chart goes.
      for (const other of catalog.dashboards.filter(board => board.charts.some(placement => placement.chart_id === chartId))) {
        await saveDashboard({ ...other, charts: other.charts.filter(placement => placement.chart_id !== chartId) });
        catalog = await getCatalog(signal);
      }
      await deleteObject("chart", chart.id, chart.revision);
      catalog = await getCatalog(signal);
    } catch (error) { failures.push(`${chart.name}：${message(error)}`); }
  }
  return failures;
}

/** Says what disappears, and calls out cards that other boards would lose too. */
function deleteWarning(target: Dashboard, catalog?: Catalog): string {
  const body = t("dashboards.delete.confirm", { name: target.name, count: target.charts.length });
  const shared = catalog ? target.charts.filter(placement => catalog.dashboards.some(board => board.id !== target.id && board.charts.some(other => other.chart_id === placement.chart_id))).length : 0;
  return shared ? `${body} ${t("dashboards.delete.shared", { shared })}` : body;
}

/** One empty state shape for both pages: a line of text plus the action that fixes it. */
function emptyState(text: string, action: { label: string; onClick: () => void }): HTMLElement {
  const empty = element("div", "empty-state");
  empty.append(element("p", "muted", text));
  const cta = button(action.label); cta.onclick = action.onClick;
  empty.append(cta);
  return empty;
}

export async function dashboardsPage(root: HTMLElement, id: string | undefined, signal: AbortSignal): Promise<void> {
  let catalog = await getCatalog(signal);
  root.className = "workspace-page analytics-page";
  if (!id) {
    const createBoard = async (): Promise<void> => {
      const answer = await promptDialog({
        title: t("dashboards.new.dialog"), submitLabel: t("common.create"), signal,
        fields: [
          { label: t("dashboards.field.name"), id: "dashboard-name", value: t("dashboards.default.name"), required: true },
          { label: t("dashboards.field.description"), id: "dashboard-description", value: "" },
        ],
      });
      if (!answer) return;
      try {
        const board = await saveDashboard({ id: crypto.randomUUID(), name: answer["dashboard-name"], description: answer["dashboard-description"], charts: [], revision: 0 });
        navigate(`/dashboard/${board.id}`);
      } catch (error) { if (!signal.aborted) window.alert(message(error)); }
    };
    const drawList = (current: Catalog) => {
      const add = button(t("dashboards.new")); add.onclick = () => void createBoard();
      root.replaceChildren(pageHeader(t("dashboards.list.title"), undefined, add));
      const boards = [...current.dashboards].sort((a, b) => a.name.localeCompare(b.name));
      if (!boards.length) { root.append(emptyState(t("app.empty.dashboards"), { label: t("dashboards.new"), onClick: () => void createBoard() })); return; }
      const list = element("div", "analytics-object-list");
      for (const board of boards) {
        const card = element("div", "card analytics-object");
        const link = element("a", "analytics-object-main"); link.href = `/dashboard/${board.id}`;
        link.append(element("h2", "", board.name), element("p", "muted", board.description), element("span", "status", tn("dashboards.card.count.one", "dashboards.card.count", board.charts.length)));
        card.append(link, cardActions(card, [
          { label: t("dashboards.card.settings"), icon: cardIcons.edit, onClick: () => void editBoard(board) },
          { label: t("dashboards.card.delete"), icon: cardIcons.remove, onClick: () => void removeBoard(board) },
        ]));
        list.append(card);
      }
      root.append(list);
    };
    const refreshList = async () => { catalog = await getCatalog(signal); if (!signal.aborted) drawList(catalog); };
    // Card actions edit or delete a saved board from the list; both reuse the board's own settings dialog.
    const editBoard = (target: Dashboard) => dashboardSettings(target, signal, async next => {
      try { await saveDashboard({ ...target, name: next.name, description: next.description }); await refreshList(); }
      catch (error) { if (!signal.aborted) window.alert(message(error)); }
    });
    const removeBoard = async (target: Dashboard) => {
      const confirmed = await confirmDialog({
        title: t("dashboards.delete.dialog"), body: deleteWarning(target, catalog), confirmLabel: t("dashboards.delete.action"), signal,
      });
      if (!confirmed) return;
      try {
        const failures = await deleteBoardWithContents(target, signal);
        await refreshList();
        if (failures.length && !signal.aborted) window.alert(failures.join("\n"));
      } catch (error) { if (!signal.aborted) window.alert(message(error)); }
    };
    drawList(catalog);
    return;
  }

  const reloadCatalog = async () => { catalog = await getCatalog(signal); if (!signal.aborted) await draw(); };
  const editChart = (chart: Chart) => chartEditor(catalog, chart, signal, async savedChart => {
    catalog.charts = catalog.charts.map(c => c.id === savedChart.id ? savedChart : c); await draw();
  }, reloadCatalog);
  let saved = catalog.dashboards.find(d => d.id === id);
  if (!saved) throw new Error(t("dashboards.missing"));
  let board: Dashboard = structuredClone(saved), refreshing = false, presenting = false;
  let grid: GridStack | undefined, renderVersion = 0;
  let cleanups: (() => void)[] = [];
  const dataCache = new Map<string, Promise<ChartData>>();
  const back = element("a", "dashboard-back"); back.href = "/dashboards"; back.textContent = t("dashboards.back");
  const header = element("div", "dashboard-heading"), title = element("div", "dashboard-title"), actions = element("div", "dashboard-actions");
  const status = element("div", "dashboard-status"), chip = statusChip(status);
  title.append(back, heading(board.name, board.description), status);
  const refresh = iconLabelButton(t("dashboards.action.refresh"), cardIcons.refresh);
  const present = iconLabelButton(t("dashboards.action.present"), cardIcons.play);
  const add = button(t("dashboards.action.addChart"));
  const more = resultIcon(t("dashboards.action.more"), cardIcons.more);
  actions.append(present, refresh, add, more);
  const feedback = element("p", "feedback"), layout = element("div", "grid-stack dashboard-grid");
  header.append(title, actions); root.append(header, feedback, layout);
  const abort = () => { grid?.destroy(false); cleanups.forEach(fn => fn()); clearTimeout(saveTimer); };
  signal.addEventListener("abort", abort, { once: true });
  const dataFor = (chart: Chart) => {
    const snapshot = chart.snapshot_id;
    if (!snapshot) return Promise.reject(new Error(t("dashboards.card.noSnapshot")));
    const cacheKey = `${snapshot}:${chart.statement}:${chart.result}`;
    if (!dataCache.has(cacheKey)) dataCache.set(cacheKey, loadChartData(snapshot, chart.statement, chart.result, signal));
    return dataCache.get(cacheKey)!;
  };
  function captureLayout() {
    if (!grid) return;
    board.charts = (grid.save(false, false, undefined, 12) as GridStackWidget[]).map(node => ({ chart_id: node.id!, x: node.x ?? 0, y: node.y ?? 0, w: node.w ?? node.minW ?? 1, h: node.h ?? node.minH ?? 1 }));
  }
  function controls() {
    refresh.disabled = refreshing;
    present.querySelector("span")!.textContent = presenting ? t("dashboards.action.exitPresent") : t("dashboards.action.present");
    // Only the heading node is replaced: the back link and status chip live in the same container.
    const page = heading(board.name, board.description), previous = title.querySelector(".page-heading");
    if (previous) previous.replaceWith(page); else title.insertBefore(page, status);
  }
  async function draw() {
    const ticket = ++renderVersion;
    grid?.destroy(false); grid = undefined; cleanups.forEach(fn => fn()); cleanups = [];
    layout.replaceChildren(); controls();
    const placements = board.charts.map(placement => ({ placement, chart: catalog.charts.find(c => c.id === placement.chart_id)! })).filter(entry => entry.chart);
    // Every chart can be re-run: it owns the SQL that produces its snapshot.
    const live = new Set(placements.map(entry => entry.chart.id));
    refresh.disabled = refreshing || live.size === 0;
    refresh.title = t("dashboards.refresh.live", { count: live.size });
    if (!placements.length) {
      layout.append(emptyState(t("dashboards.empty.title"), { label: t("dashboards.action.addChart"), onClick: () => add.click() }));
      return;
    }
    const loaded = await Promise.all(placements.map(async entry => {
      try { return { ...entry, data: await dataFor(entry.chart), error: "" }; }
      catch (error) { return { ...entry, data: undefined, error: message(error) }; }
    }));
    if (ticket !== renderVersion || signal.aborted) return;
    for (const entry of loaded) {
      const { chart, placement, data } = entry;
      const item = element("div", "grid-stack-item");
      for (const [attribute, value] of Object.entries({ id: chart.id, x: placement.x, y: placement.y, w: placement.w, h: placement.h, "min-w": 3, "min-h": 2 })) item.setAttribute(`gs-${attribute}`, String(value));
      const card = element("section", "grid-stack-item-content chart-card"), head = element("div", "chart-card-heading");
      // Only this grip starts a drag: the chart body stays free for tooltips, legends and canvas interaction.
      const grip = element("button", "chart-card-grip");
      grip.type = "button"; grip.title = t("dashboards.card.drag"); grip.setAttribute("aria-label", t("dashboards.card.drag"));
      grip.append(svgIcon(cardIcons.drag));
      head.append(element("h2", "", chart.name), grip);
      const tools = cardActions(card, [
        { label: t("dashboards.card.edit"), icon: cardIcons.edit, onClick: () => editChart(chart) },
        { label: t("dashboards.card.remove"), icon: cardIcons.remove, onClick: () => { captureLayout(); board.charts = board.charts.filter(placement => placement.chart_id !== chart.id); void draw().then(persist); } },
      ], 1800);
      head.append(tools);
      // One quiet line carries the whole provenance of the card; errors replace it in place.
      const meta = element("p", "card-meta");
      if (data) meta.textContent = tn("dashboards.card.meta.one", "dashboards.card.meta", data.rows.length, { type: chartLabels[chart.spec.chart_type], time: new Date(data.metadata.created_at * 1000).toLocaleString(), rows: data.rows.length });
      else meta.textContent = t("dashboards.card.metaPending", { type: chartLabels[chart.spec.chart_type] });
      meta.title = meta.textContent;
      if (!data && entry.error) meta.classList.add("error");
      const canvas = element("div", "chart-canvas");
      if (data) {
        requestAnimationFrame(() => {
          if (ticket !== renderVersion || signal.aborted) return;
          try { cleanups.push(renderChart(canvas, chart.spec, data)); }
          catch (error) { canvas.replaceChildren(element("p", "feedback error", message(error))); }
        });
      } else canvas.append(element("p", "feedback error", entry.error));
      card.append(head, meta, canvas); item.append(card); layout.append(item);
    }
    requestAnimationFrame(() => {
      if (ticket !== renderVersion || signal.aborted || !loaded.length) return;
      // gridstack only starts a drag when the mousedown target is the handle itself, so the grip's icon must not swallow the
      // event (CSS pointer-events: none) and the default button/input cancel list must not match it.
      grid = GridStack.init({ column: 12, cellHeight: 64, margin: 8, staticGrid: presenting, animate: false, draggable: { handle: ".chart-card-grip", cancel: "sqlx-no-cancel" }, resizable: { handles: "se" }, columnOpts: { breakpoints: [{ w: 700, c: 1 }] } }, layout);
      grid.on("change", () => { captureLayout(); scheduleSave(); });
    });
  }
  let saveTimer: ReturnType<typeof setTimeout> | undefined, saving = false;
  const scheduleSave = () => { clearTimeout(saveTimer); saveTimer = setTimeout(() => void persist(), 700); };
  // Layout edits persist on their own; the header chip reports the outcome without moving the grid.
  async function persist() {
    if (saving) { scheduleSave(); return; }
    saving = true; captureLayout();
    try {
      saved = await saveDashboard(board); board = structuredClone(saved);
      chip.ok(t("dashboards.status.saved", { time: new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) }));
    } catch (error) {
      chip.fail(t("dashboards.status.failed", { error: message(error) }), () => void persist());
      try { catalog = await getCatalog(signal); const remote = catalog.dashboards.find(d => d.id === id); if (remote) { saved = remote; board = structuredClone(remote); } if (!signal.aborted) await draw(); } catch { /* keep the local draft when the reload itself fails */ }
    } finally { saving = false; }
  }
  const boardSettings = () => { captureLayout(); dashboardSettings(board, signal, next => { board = next; void draw().then(persist); }); };
  const removeBoard = async () => {
    const confirmed = await confirmDialog({
      title: t("dashboards.delete.dialog"), body: deleteWarning(board, catalog), confirmLabel: t("dashboards.delete.action"), signal,
    });
    if (!confirmed) return;
    try {
      const failures = await deleteBoardWithContents(board, signal);
      if (failures.length) chip.fail(failures.join("；"));
      navigate("/dashboards");
    } catch (error) { chip.fail(message(error)); }
  };
  overflowMenu(more, [
    { label: t("dashboards.card.settings"), onClick: boardSettings },
    { label: t("dashboards.card.delete"), danger: true, onClick: () => void removeBoard() },
  ], signal);
  // A chart belongs to the board it was created on, so adding one always means writing a new one.
  add.onclick = () => {
    captureLayout();
    const append = async (chart: Chart) => {
      if (!catalog.charts.some(c => c.id === chart.id)) catalog.charts.push(chart);
      const y = Math.max(0, ...board.charts.map(placement => placement.y + placement.h));
      board.charts.push({ chart_id: chart.id, x: 0, y, w: 6, h: 4 });
      await draw(); await persist();
    };
    try { chartEditor(catalog, undefined, signal, append, reloadCatalog); }
    catch (error) { chip.fail(message(error)); }
  };
  refresh.onclick = async () => {
    if (refreshing) return; refreshing = true; refresh.disabled = true;
    chip.ok(t("dashboards.refresh.running"));
    const chartIds = board.charts.map(placement => placement.chart_id);
    try {
      const outcomes = await Promise.allSettled(chartIds.map(async chartId => waitRun(await runChart(chartId), signal)));
      if (signal.aborted) return;
      catalog = await getCatalog(signal);
      const failures = outcomes.flatMap((outcome, index) => outcome.status === "rejected" ? [`${catalog.charts.find(c => c.id === chartIds[index])?.name ?? chartIds[index]}：${message(outcome.reason)}`] : []);
      if (failures.length) chip.fail(failures.join("；"));
      else chip.ok(t("dashboards.refresh.done", { count: chartIds.length }));
      await draw();
    } catch (error) { if (!signal.aborted) chip.fail(message(error)); }
    finally { refreshing = false; refresh.disabled = false; }
  };
  // Presentation is read-only: the grid stops accepting drags and resizes while it is on.
  // The page element is rendered offscreen and copied in, so presentation state lives on <body>.
  const presentation = (on: boolean) => {
    presenting = on;
    document.body.classList.toggle("dashboard-presenting", on);
    grid?.setStatic(on);
    controls();
  };
  present.onclick = () => presentation(!presenting);
  const exit = (event: KeyboardEvent) => { if (event.key === "Escape" && presenting) presentation(false); };
  document.addEventListener("keydown", exit);
  signal.addEventListener("abort", () => { document.body.classList.remove("dashboard-presenting"); document.removeEventListener("keydown", exit); }, { once: true });
  await draw();
}
