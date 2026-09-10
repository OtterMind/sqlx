import { api, authenticate } from "./api";
import { element, heading, message, statusBadge } from "./components";
import { setupPage } from "./setup";
import { resultPage } from "./result";

interface Entry {
  id: string;
  name: string;
  status: string;
  created_at?: number;
}
async function home(root: HTMLElement) {
  const data = await api<{ setups: Entry[]; results: Entry[] }>("/home");
  root.replaceChildren(
    heading(
      "LOCAL WORKSPACE",
      "Your database, in view.",
      "Complete a connection request or return to a recent query result.",
    ),
  );
  for (const [title, entries, kind] of [
    ["Connection requests", data.setups, "setup"],
    ["Query results", data.results, "result"],
  ] as const) {
    const section = element("section", "card home-section");
    section.append(element("h2", "", title));
    if (!entries.length)
      section.append(
        element(
          "p",
          "muted",
          kind === "setup"
            ? "Ask your agent to create a connection with --ui."
            : "Run a query with --view to see its results here.",
        ),
      );
    for (const entry of [...entries].sort(
      (a, b) => (b.created_at ?? 0) - (a.created_at ?? 0),
    )) {
      const link = element("a", "home-entry");
      link.href = `/${kind}/${entry.id}`;
      link.append(element("span", "", entry.name), statusBadge(entry.status));
      section.append(link);
    }
    root.append(section);
  }
}
async function start() {
  const root = document.getElementById("app")!;
  try {
    await authenticate();
    const route = location.pathname.split("/").filter(Boolean);
    if (route[0] === "setup" && route[1]) await setupPage(root, route[1]);
    else if (route[0] === "result" && route[1])
      await resultPage(root, route[1]);
    else await home(root);
  } catch (error) {
    root.replaceChildren(
      heading(
        "SQLX",
        "This page is unavailable",
        "Open a new page from your terminal or agent.",
      ),
      element("p", "feedback error", message(error)),
    );
  }
}
void start();
