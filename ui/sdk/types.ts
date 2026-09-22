/** Browser API v1. No saved password or local encryption key is returned by these APIs. */
export const UI_API_VERSION = 1;
export type DatabaseType =
  | "mysql"
  | "mariadb"
  | "tidb"
  | "greatsql"
  | "oceanbase"
  | "postgresql"
  | "cockroachdb"
  | "yugabytedb"
  | "opengauss"
  | "oracle"
  | "sqlserver"
  | "clickhouse"
  | "trino"
  | "starrocks"
  | "doris"
  | "tdengine"
  | "dameng"
  | "kingbase"
  | "redis"
  | "mongodb";
export interface ConnectionFields {
  database_type: DatabaseType;
  host: string;
  port: number;
  database: string;
  service: string;
  username: string;
  tls: "disable" | "verify-full";
}
export interface SetupStatus {
  request_id: string;
  status: "waiting_for_user" | "saving" | "completed" | "cancelled" | "expired";
  error: string | null;
  datasource_id: string | null;
}
export interface SetupView extends SetupStatus {
  name: string;
  connection: ConnectionFields;
  editing: boolean;
}
export interface SetupSubmission {
  name: string;
  connection: ConnectionFields & { password: string };
  password_action: "keep" | "replace" | "clear";
}
export interface Column {
  name: string;
  database_type: string;
  encoding: string;
}
export interface ResultTable {
  statement: number;
  result: number;
  columns: Column[];
  rows: number;
  affected_rows: string | null;
  complete: boolean;
}
export interface ResultEvent {
  event: string;
  index?: number | null;
  message?: string;
  outcome?: string;
  code?: string;
}
export interface ResultMetadata {
  result_id: string;
  datasource_id: string;
  datasource_name: string;
  statements: string[];
  created_at: number;
  status:
    "queued" | "running" | "completed" | "failed" | "cancelled" | "interrupted";
  duration_ms: number;
  tables: ResultTable[];
  events: ResultEvent[];
  snapshot: string | null;
  refresh: ResultRefresh | null;
}
export interface ResultRefresh {
  request_id: string;
  status: "running" | "completed" | "failed" | "cancelled" | "interrupted";
  error: string | null;
}
export interface ResultPage {
  rows: unknown[][];
  offset: number;
  next_offset: number;
  total_rows: number;
  complete: boolean;
}
export interface WorkspaceEntry {
  id: string;
  name: string;
  status: string;
  created_at?: number;
}
export interface Workspace {
  datasources: Datasource[];
  setups: WorkspaceEntry[];
  results: WorkspaceEntry[];
}

export interface Datasource {
  id: string;
  name: string;
  connection: Omit<ConnectionFields, "username">;
}

export interface ConnectionTest {
  connected: boolean;
  duration_ms: number;
}

export interface PluginManifest {
  schema_version: 1;
  id: string;
  name: string;
  version: string;
  api_version: 1;
  cli_compat: string;
  entrypoint: string;
  capabilities: string[];
  description: string;
}
