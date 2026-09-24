import { api } from "./api";
import { editDatasource, testDatasource } from "../sdk/client";
import type { DatabaseType, Datasource } from "../sdk/types";
import { button, element, heading, message } from "./components";
import { navigate } from "./navigation";

const databaseNames: Record<DatabaseType, string> = {
  mysql: "MySQL",
  mariadb: "MariaDB",
  tidb: "TiDB",
  greatsql: "GreatSQL",
  oceanbase: "OceanBase",
  postgresql: "PostgreSQL",
  cockroachdb: "CockroachDB",
  yugabytedb: "YugabyteDB",
  opengauss: "openGauss",
  oracle: "Oracle",
  sqlserver: "SQL Server",
  clickhouse: "ClickHouse",
  trino: "Trino",
  starrocks: "StarRocks",
  doris: "Apache Doris",
  tdengine: "TDengine",
  dameng: "Dameng",
  kingbase: "KingbaseES",
  redis: "Redis",
  mongodb: "MongoDB",
  sqlite: "SQLite",
  duckdb: "DuckDB",
  h2: "H2",
  presto: "Presto",
  hive: "Hive",
  kylin: "Apache Kylin",
  xugu: "XuguDB",
  db2: "IBM Db2",
  informix: "IBM Informix",
  sundb: "SUNDB",
  gbase8s: "GBase 8s",
};
/** Engines that open a local file, or a local file until a host is given. */
export function isFileEngine(kind: string): boolean {
  return kind === "sqlite" || kind === "duckdb";
}

export function datasourceList(
  sources: Datasource[],
  compact = false,
): HTMLElement {
  const section = element(
    "section",
    compact ? "sidebar-section" : "card home-section",
  );
  const header = element("div", "section-heading");
  header.append(
    element("h2", "", "Datasources"),
    element("span", "entry-count", String(sources.length)),
  );
  section.append(header);
  if (!sources.length)
    section.append(element("p", "muted", "No saved datasources"));
  for (const source of [...sources].sort((a, b) =>
    a.name.localeCompare(b.name),
  )) {
    const link = element("a", compact ? "sidebar-entry" : "home-entry");
    link.href = `/datasource/${source.id}`;
    if (location.pathname === link.pathname)
      link.setAttribute("aria-current", "page");
    const name = element("span", "entry-name", source.name);
    name.title = source.name;
    const connection = source.connection;
    const context = element(
      "span",
      "entry-context datasource-context",
      isFileEngine(connection.database_type)
        ? `${databaseNames[connection.database_type]} · ${connection.database}`
        : `${databaseNames[connection.database_type]} · ${connection.host}:${connection.port}`,
    );
    context.title = context.textContent ?? "";
    link.append(name, context);
    section.append(link);
  }
  return section;
}

export async function datasourcePage(
  root: HTMLElement,
  id: string,
  signal: AbortSignal,
): Promise<void> {
  const source = await api<Datasource>(
    `/datasources/${encodeURIComponent(id)}`,
    undefined,
    signal,
  );
  signal.throwIfAborted();
  root.className = "setup-page";
  const c = source.connection;
  const card = element("section", "card setup-card");
  const details = element("dl", "datasource-details");
  const file = isFileEngine(c.database_type) || (c.database_type === "h2" && !c.host);
  const values: [string, string][] = file
    ? [
        ["Database type", databaseNames[c.database_type]],
        ["Database file", c.database],
      ]
    : [
        ["Database type", databaseNames[c.database_type]],
        ["Host", c.host],
        ["Port", String(c.port)],
        [
          c.database_type === "oracle" || c.database_type === "informix" || c.database_type === "gbase8s"
            ? "Service name"
            : "Database",
          c.database_type === "oracle" || c.database_type === "informix" || c.database_type === "gbase8s"
            ? c.service
            : c.database || "Not specified",
        ],
        [
          "Connection security",
          c.tls === "disable" ? "TLS disabled" : "Verify server certificate",
        ],
      ];
  for (const [label, value] of values)
    details.append(element("dt", "", label), element("dd", "", value));
  const actions = element("div", "actions");
  const test = button("Test connection", "button secondary");
  const edit = button("Edit connection");
  const feedback = element("p", "feedback");
  feedback.setAttribute("role", "status");
  actions.append(test, edit);
  card.append(details, actions, feedback);
  root.replaceChildren(heading(source.name, "Saved datasource"), card);

  async function run(action: "test" | "edit") {
    if (signal.aborted || test.disabled) return;
    test.disabled = edit.disabled = true;
    feedback.className = "feedback";
    feedback.textContent =
      action === "test"
        ? "Testing connection…"
        : "Opening connection settings…";
    try {
      if (action === "test") {
        const result = await testDatasource(id);
        if (!signal.aborted)
          feedback.textContent = `Connected successfully (${result.duration_ms} ms).`;
      } else if (action === "edit") {
        const setup = await editDatasource(id);
        if (!signal.aborted) navigate(`/setup/${setup.request_id}`);
      }
    } catch (error) {
      if (!signal.aborted) {
        feedback.className = "feedback error";
        feedback.textContent = message(error);
      }
    } finally {
      if (!signal.aborted) test.disabled = edit.disabled = false;
    }
  }
  test.onclick = () => void run("test");
  edit.onclick = () => void run("edit");
}
