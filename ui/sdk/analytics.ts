import { api } from "./client";
import { t } from "../src/i18n";
import type { ResultMetadata, ResultPage } from "./types";

export const chartTypes = ["Column", "Bar", "Line", "AreaLine", "Scatter", "Pie", "RingPie", "RosePie", "Funnel", "WordCloud", "Statistics", "Combo", "Table"] as const;
export type ChartType = typeof chartTypes[number];
export type SeriesType = "Column" | "Line" | "AreaLine" | "Scatter";
export interface Metric { field: string; label: string; unit: string; axis: "left" | "right"; kind: SeriesType | null }
export interface ChartSpec { chart_type: ChartType; dimension: string | null; group_by: string[]; metrics: Metric[]; stack: boolean; line_style: "straight" | "smooth" | "step" }
export interface Chart { id: string; name: string; datasource_id: string; statements: string[]; statement: number; result: number; snapshot_id: string | null; history: string[]; spec: ChartSpec; revision: number }
export interface Placement { chart_id: string; x: number; y: number; w: number; h: number }
export interface Dashboard { id: string; name: string; description: string; charts: Placement[]; revision: number }
export interface ChartRun { request_id: string; chart_id: string; status: "running" | "completed" | "failed"; error: string | null }
export interface Catalog { schema_version: number; charts: Chart[]; dashboards: Dashboard[]; runs: ChartRun[] }
export interface ChartData { metadata: ResultMetadata; columns: string[]; rows: unknown[][] }
export const getCatalog = (signal?: AbortSignal) => api<Catalog>("/analytics", undefined, signal);
export const saveChart = (value: Chart) => api<Chart>("/analytics/charts", value);
export const saveDashboard = (value: Dashboard) => api<Dashboard>("/analytics/dashboards", value);
export const runChart = (id: string) => api<ChartRun>(`/analytics/charts/${id}/run`, { request_id: crypto.randomUUID() });
export const deleteObject = (kind: "chart" | "dashboard", id: string, revision: number) => api("/analytics/delete", { kind, id, revision });
export const getSnapshot = (id: string, signal?: AbortSignal) => api<ResultMetadata>(`/analytics/snapshots/${id}`, undefined, signal);

/** Fetch every row from one immutable result set; charts never use the CLI's printed preview. */
export async function loadChartData(id: string, statement: number, result: number, signal?: AbortSignal): Promise<ChartData> {
  const metadata = await getSnapshot(id, signal);
  const table = metadata.tables.find(t => t.statement === statement && t.result === result);
  if (!table?.complete) throw new Error(t("analytics.error.snapshotIncomplete"));
  if (table.rows > 100_000) throw new Error(t("analytics.error.tooManyRows"));
  const rows: unknown[][] = [];
  let offset = 0;
  while (offset < table.rows) {
    const page = await api<ResultPage>(`/analytics/snapshots/${id}/rows?statement=${statement}&result=${result}&offset=${offset}&limit=200`, undefined, signal);
    if (page.next_offset <= offset) throw new Error(t("analytics.error.snapshotMissing"));
    rows.push(...page.rows); offset = page.next_offset;
  }
  return { metadata, columns: table.columns.map(c => c.name), rows };
}
