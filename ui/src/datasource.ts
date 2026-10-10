import { api } from "./api";
import { editDatasource, testDatasource } from "../sdk/client";
import type { ConnectionTest, DatabaseType, Datasource, SetupStatus } from "../sdk/types";
import { button, element, heading, message } from "./components";
import { t } from "./i18n";
import { navigate } from "./navigation";

export const databaseNames: Record<DatabaseType, string> = {
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
  // The heading is the way into the datasource overview, matching how the Dashboards entry works.
  const header = element("a", compact ? "section-heading sidebar-overview" : "section-heading");
  header.href = "/datasources";
  if (location.pathname === header.pathname) header.setAttribute("aria-current", "page");
  header.append(
    element("h2", "", t("app.nav.datasources")),
    element("span", "entry-count", String(sources.length)),
  );
  section.append(header);
  if (!sources.length)
    section.append(element("p", "muted", t("app.empty.datasources")));
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

/** Opens the connection form for a saved datasource; the caller navigates to the returned setup request. */
export function openConnectionSettings(id: string): Promise<SetupStatus> {
  return editDatasource(id);
}
/** Tests a saved datasource without touching stored credentials. */
export function testDatasourceConnection(id: string): Promise<ConnectionTest> {
  return testDatasource(id);
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
        [t("setup.databaseType"), databaseNames[c.database_type]],
        [t("setup.databaseFile"), c.database],
      ]
    : [
        [t("setup.databaseType"), databaseNames[c.database_type]],
        [t("setup.host"), c.host],
        [t("setup.port"), String(c.port)],
        [
          c.database_type === "oracle" || c.database_type === "informix" || c.database_type === "gbase8s"
            ? t("setup.service")
            : t("setup.database"),
          c.database_type === "oracle" || c.database_type === "informix" || c.database_type === "gbase8s"
            ? c.service
            : c.database || t("datasource.notSpecified"),
        ],
        [
          t("setup.security"),
          c.tls === "disable" ? t("setup.security.tlsOff") : t("setup.security.verify"),
        ],
      ];
  for (const [label, value] of values)
    details.append(element("dt", "", label), element("dd", "", value));
  const actions = element("div", "actions");
  const test = button(t("datasource.test"), "button secondary");
  const edit = button(t("datasource.edit"));
  const feedback = element("p", "feedback");
  feedback.setAttribute("role", "status");
  actions.append(test, edit);
  card.append(details, actions, feedback);
  root.replaceChildren(heading(source.name, t("datasource.saved")), card);

  async function run(action: "test" | "edit") {
    if (signal.aborted || test.disabled) return;
    test.disabled = edit.disabled = true;
    feedback.className = "feedback";
    feedback.textContent =
      action === "test"
        ? t("datasource.testing")
        : t("datasource.opening");
    try {
      if (action === "test") {
        const result = await testDatasourceConnection(id);
        if (!signal.aborted)
          feedback.textContent = t("datasource.test.ok", { ms: result.duration_ms });
      } else if (action === "edit") {
        const setup = await openConnectionSettings(id);
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
