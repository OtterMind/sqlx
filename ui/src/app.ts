import { api, authenticate, ServiceUnavailableError } from "./api";
import { button, element, heading, message, statusBadge } from "./components";
import { setupPage } from "./setup";
import { resultPage } from "./result";
import { initializeTheme } from "./theme";
import { startNavigation } from "./navigation";
import { keepConnectionAlive } from "./connection";
import { datasourceList, datasourcePage } from "./datasource";

import type { Workspace, WorkspaceEntry } from "../sdk/types";

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
  const header = element("div", "section-heading");
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
        kind === "setup" ? "No connection requests" : "No query results yet",
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
function renderSidebar(data: Workspace) {
  const sidebar = document.getElementById("sidebar")!;
  const overview = element("a", "workspace-overview", "Workspace");
  overview.href = "/";
  if (location.pathname === "/") overview.setAttribute("aria-current", "page");
  sidebar.replaceChildren(
    overview,
    datasourceList(data.datasources, true),
    ...pendingRequests(data, true),
    entryList("Query history", data.results, "result", true),
  );
}
function pendingRequests(data: Workspace, compact = false): HTMLElement[] {
  const waiting = data.setups.filter((entry) =>
    ["waiting_for_user", "saving"].includes(entry.status),
  );
  return waiting.length
    ? [entryList("Connection requests", waiting, "setup", compact)]
    : [];
}
function homePage(root: HTMLElement, data: Workspace) {
  root.className = "workspace-page";
  root.replaceChildren(
    heading(
      "Workspace",
      "Select a saved datasource or query result to get started.",
    ),
    datasourceList(data.datasources),
    ...pendingRequests(data),
    entryList("Recent results", data.results, "result"),
  );
}

async function start() {
  initializeTheme();
  const root = document.getElementById("app")!;
  const unavailable = (target: HTMLElement, error: unknown) => {
    const retry = button("Retry", "button secondary");
    retry.onclick = () => location.reload();
    target.className = "workspace-page";
    target.replaceChildren(
      heading(
        error instanceof ServiceUnavailableError
          ? "Connection unavailable"
          : "This page is unavailable",
        "Your saved connections and completed results are preserved.",
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
      renderSidebar(data);
      const route = url.pathname.split("/").filter(Boolean);
      const updateStatus = (entries: WorkspaceEntry[]) => (status: string) => {
        if (signal.aborted) return;
        const entry = entries.find((entry) => entry.id === route[1]);
        if (entry && entry.status !== status) {
          entry.status = status;
          renderSidebar(data);
          if (status === "completed") {
            void api<Workspace>("/home", undefined, signal)
              .then((next) => {
                if (signal.aborted) return;
                data = next;
                renderSidebar(data);
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
      else if (url.pathname === "/") homePage(pane, data);
      else throw new Error("This page does not exist.");
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
