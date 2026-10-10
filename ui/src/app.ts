import { dashboardsPage } from "./dashboards";
import { api, authenticate, ServiceUnavailableError } from "./api";
import { button, cardActions, cardIcons, element, heading, inlineNotice, message, pageHeader, statusBadge, statusText } from "./components";
import { navigate } from "./navigation";
import { setupPage } from "./setup";
import { resultPage } from "./result";
import { initializeTheme } from "./theme";
import { startNavigation } from "./navigation";
import { keepConnectionAlive } from "./connection";
import { initializeLanguage, initializeShellText } from "./language";
import { t, tn } from "./i18n";
import {
  databaseNames,
  datasourceList,
  datasourcePage,
  isFileEngine,
  openConnectionSettings,
  testDatasourceConnection,
} from "./datasource";

import type { Catalog } from "../sdk/analytics";
import type { Datasource, Workspace, WorkspaceEntry } from "../sdk/types";

function entryList(
  title: string,
  entries: WorkspaceEntry[],
  kind: "setup" | "result",
  compact = false,
) {
  const section = element(
    "section",
    compact ? "sidebar-section" : "card home-section",
  );
  // The heading is the way into the overview, matching how the Dashboards entry works.
  const header = element("a", compact ? "section-heading sidebar-overview" : "section-heading");
  header.href = kind === "result" ? "/results" : "/setups";
  if (location.pathname === header.pathname) header.setAttribute("aria-current", "page");
  header.append(
    element("h2", "", title),
    element("span", "entry-count", String(entries.length)),
  );
  section.append(header);
  if (!entries.length)
    section.append(
      element(
        "p",
        "muted",
        kind === "setup" ? t("app.empty.setups") : t("app.empty.results"),
      ),
    );
  for (const entry of [...entries].sort(
    (a, b) => (b.created_at ?? 0) - (a.created_at ?? 0),
  )) {
    const link = element("a", compact ? "sidebar-entry" : "home-entry");
    link.href = `/${kind}/${entry.id}`;
    if (location.pathname === link.pathname)
      link.setAttribute("aria-current", "page");
    const name = element("span", "entry-name", entry.name);
    name.title = entry.name;
    const context = element("span", "entry-context");
    context.append(statusBadge(entry.status));
    if (entry.created_at)
      context.append(
        element(
          "time",
          "entry-time",
          new Date(entry.created_at * 1000).toLocaleTimeString([], {
            hour: "2-digit",
            minute: "2-digit",
          }),
        ),
      );
    link.append(name, context);
    section.append(link);
  }
  return section;
}
/** The overview link already names the list and carries the count, so the rows sit directly beneath it. */
async function dashboardShortcuts(signal: AbortSignal): Promise<HTMLElement> {
  const section = element("section", "sidebar-section sidebar-dashboards");
  try {
    const catalog = await api<Catalog>("/analytics", undefined, signal);
    if (signal.aborted) return section;
    section.dataset.count = String(catalog.dashboards.length);
    if (!catalog.dashboards.length) section.append(element("p", "muted", t("app.empty.dashboards")));
    for (const board of catalog.dashboards) {
      const link = element("a", "sidebar-entry");
      link.href = `/dashboard/${board.id}`;
      if (location.pathname === link.pathname) link.setAttribute("aria-current", "page");
      // The list under Dashboards mirrors the cards: name plus the board's own description.
      const fallback = tn("dashboards.card.count.one", "dashboards.card.count", board.charts.length);
      const description = element("span", "entry-context", board.description || fallback);
      description.title = board.description || fallback;
      link.append(element("span", "entry-name", board.name), description);
      section.append(link);
    }
  } catch (error) {
    if (!signal.aborted) section.append(element("p", "muted", message(error)));
  }
  return section;
}
async function renderSidebar(data: Workspace, signal: AbortSignal) {
  const sidebar = document.getElementById("sidebar")!;
  const dashboards = element("a", "workspace-overview", t("app.nav.dashboards")); dashboards.href = "/dashboards";
  if (location.pathname === "/" || location.pathname === "/dashboards") dashboards.setAttribute("aria-current", "page");
  const boards = await dashboardShortcuts(signal);
  if (signal.aborted) return;
  const count = boards.dataset.count;
  dashboards.append(element("span", "entry-count", count ?? ""));
  const sources = datasourceList(data.datasources, true), history = entryList(t("app.nav.queryHistory"), data.results, "result", true);
  // Every top-level entry owns an overview page, so a click on the label opens the list instead of a dead end.
  sidebar.replaceChildren(
    dashboards,
    boards,
    sources,
    ...pendingRequests(data, true),
    history,
  );
}
/** A compact connection summary for the every-datasource overview. */
function connectionSummary(source: Datasource): string {
  const c = source.connection;
  return isFileEngine(c.database_type)
    ? `${databaseNames[c.database_type]} · ${c.database}`
    : `${databaseNames[c.database_type]} · ${c.host}:${c.port}${c.database ? ` · ${c.database}` : ""}`;
}
/** Every saved datasource, opened from the sidebar heading; the cards match the dashboard list. */
export function datasourcesPage(root: HTMLElement, data: Workspace, signal: AbortSignal, onChanged: (next: Workspace) => void): void {
  root.className = "workspace-page analytics-page";
  root.replaceChildren(pageHeader(t("app.nav.datasources"), t("app.nav.datasources.hint")));
  const list = element("div", "analytics-object-list");
  const reindex = (next: Workspace) => onChanged(next);
  for (const source of [...data.datasources].sort((a, b) => a.name.localeCompare(b.name))) {
    const card = element("div", "card analytics-object");
    const link = element("a", "analytics-object-main");
    link.href = `/datasource/${source.id}`;
    link.append(
      element("h2", "", source.name),
      element("p", "muted", connectionSummary(source)),
    );
    card.append(link, cardActions(card, [
      { label: t("datasource.test"), icon: cardIcons.plug, onClick: async () => {
        try { const result = await testDatasourceConnection(source.id); inlineNotice(card, t("datasource.test.ok", { ms: result.duration_ms })); }
        catch (error) { inlineNotice(card, message(error), true); }
      } },
      { label: t("datasource.edit"), icon: cardIcons.edit, onClick: async () => {
        try { const setup = await openConnectionSettings(source.id); reindex(await api<Workspace>("/home", undefined, signal)); navigate(`/setup/${setup.request_id}`); }
        catch (error) { inlineNotice(card, message(error), true); }
      } },
    ], 1600, signal));
    list.append(card);
  }
  if (!data.datasources.length) list.append(element("p", "muted", t("app.empty.datasources")));
  root.append(list);
}
/** Every stored query result, opened from the Query history heading. */
export function resultsPage(root: HTMLElement, data: Workspace): void {
  root.className = "workspace-page analytics-page";
  root.replaceChildren(pageHeader(t("app.nav.queryHistory"), t("app.nav.queryHistory.hint")));
  const list = element("div", "analytics-object-list");
  for (const entry of [...data.results].sort((a, b) => (b.created_at ?? 0) - (a.created_at ?? 0))) {
    const link = element("a", "card analytics-object");
    link.href = `/result/${entry.id}`;
    link.append(element("h2", "", entry.name));
    const meta = element("p", "muted");
    meta.append(statusBadge(entry.status));
    if (entry.created_at) meta.append(` · ${new Date(entry.created_at * 1000).toLocaleString()}`);
    link.append(meta);
    list.append(link);
  }
  if (!data.results.length) list.append(element("p", "muted", t("app.empty.results")));
  root.append(list);
}
/** Connection requests that still need an answer. */
export function setupsPage(root: HTMLElement, data: Workspace): void {
  root.className = "workspace-page analytics-page";
  root.replaceChildren(pageHeader(t("app.nav.connectionRequests"), t("app.nav.connectionRequests.hint")));
  const list = element("div", "analytics-object-list");
  for (const entry of [...data.setups].sort((a, b) => (b.created_at ?? 0) - (a.created_at ?? 0))) {
    const link = element("a", "card analytics-object");
    link.href = `/setup/${entry.id}`;
    link.append(element("h2", "", entry.name), element("p", "muted", statusText(entry.status)));
    list.append(link);
  }
  if (!data.setups.length) list.append(element("p", "muted", t("app.empty.setups")));
  root.append(list);
}
function pendingRequests(data: Workspace, compact = false): HTMLElement[] {
  const waiting = data.setups.filter((entry) =>
    ["waiting_for_user", "saving"].includes(entry.status),
  );
  return waiting.length
    ? [entryList(t("app.nav.connectionRequests"), waiting, "setup", compact)]
    : [];
}
async function start() {
  initializeTheme();
  initializeLanguage();
  initializeShellText();
  const root = document.getElementById("app")!;
  const unavailable = (target: HTMLElement, error: unknown) => {
    const retry = button(t("common.retry"), "button secondary");
    retry.onclick = () => location.reload();
    target.className = "workspace-page";
    target.replaceChildren(
      heading(
        error instanceof ServiceUnavailableError
          ? t("app.offline.title")
          : t("app.unavailable.title"),
        t("app.unavailable.hint"),
      ),
      element("p", "feedback error", message(error)),
      retry,
    );
  };
  try {
    await authenticate();
  } catch (error) {
    unavailable(root, error);
    return;
  }
  if (location.pathname === "/") history.replaceState(null, "", "/dashboards");
  // Charts own their SQL now, so the old dataset routes only need to keep old links working.
  else if (/^\/(datasets|dataset)(\/|$)/.test(location.pathname)) {
    history.replaceState(null, "", "/dashboards");
  }
  // Switching language re-renders the current page and re-applies the shell's static labels.
  document.addEventListener("sqlx:locale", () => {
    initializeShellText();
    window.dispatchEvent(new PopStateEvent("popstate"));
  });
  startNavigation(async (url, signal) => {
    root.inert = true;
    root.setAttribute("aria-busy", "true");
    for (const dialog of document.querySelectorAll<HTMLDialogElement>(
      ".value-dialog",
    ))
      dialog.close();
    // Prepare the new pane offscreen, keeping the current view visible until ready.
    const pane = element("div");
    try {
      let data = await api<Workspace>("/home", undefined, signal);
      signal.throwIfAborted();
      await renderSidebar(data, signal);
      const route = url.pathname.split("/").filter(Boolean);
      const updateStatus = (entries: WorkspaceEntry[]) => (status: string) => {
        if (signal.aborted) return;
        const entry = entries.find((entry) => entry.id === route[1]);
        if (entry && entry.status !== status) {
          entry.status = status;
          void renderSidebar(data, signal);
          if (status === "completed") {
            void api<Workspace>("/home", undefined, signal)
              .then((next) => {
                if (signal.aborted) return;
                data = next;
                void renderSidebar(data, signal);
              })
              .catch((error) => {
                if (!signal.aborted)
                  document
                    .getElementById("sidebar")!
                    .append(element("p", "feedback error", message(error)));
              });
          }
        }
      };
      if (route[0] === "setup" && route[1])
        await setupPage(pane, route[1], updateStatus(data.setups), signal);
      else if (route[0] === "result" && route[1])
        await resultPage(pane, route[1], updateStatus(data.results), signal);
      else if (route[0] === "datasource" && route[1])
        await datasourcePage(pane, route[1], signal);
      else if (route[0] === "dashboards" || route[0] === "dashboard") await dashboardsPage(pane, route[1], signal);
      else if (url.pathname === "/datasources") datasourcesPage(pane, data, signal, next => { data = next; });
      else if (url.pathname === "/results") resultsPage(pane, data);
      else if (url.pathname === "/setups") setupsPage(pane, data);
      else if (url.pathname === "/") await dashboardsPage(pane, undefined, signal);
      else throw new Error(t("app.error.pageMissing"));
      signal.throwIfAborted();
      root.className = pane.className;
      root.replaceChildren(...pane.childNodes);
      root.scrollTop = 0;
    } catch (error) {
      if (!signal.aborted) unavailable(root, error);
    } finally {
      if (!signal.aborted) {
        root.inert = false;
        root.removeAttribute("aria-busy");
        const focus = root.querySelector<HTMLElement>("[data-initial-focus]");
        if (focus) focus.focus();
      }
    }
  });
  keepConnectionAlive();
}
void start();
