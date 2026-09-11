import type {
  PluginManifest,
  Workspace,
  SetupStatus,
  SetupSubmission,
  SetupView,
  ResultMetadata,
  ResultPage,
  Datasource,
  ConnectionTest,
  ResultRefresh,
} from "./types";
export * from "./types";

export class ServiceUnavailableError extends Error {
  constructor() {
    super(
      "Cannot reach the local SQLX service. Retry the connection. If it has stopped, run sqlx ui --no-open and open its URL in this tab.",
    );
    this.name = "ServiceUnavailableError";
  }
}

export async function api<T>(
  path: string,
  body?: unknown,
  signal?: AbortSignal,
): Promise<T> {
  const response = await fetch(`/api${path}`, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "X-SQLX-UI": "1",
      ...(body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    credentials: "same-origin",
    body: body === undefined ? undefined : JSON.stringify(body),
    cache: "no-store",
    signal,
  }).catch((error: unknown) => {
    if (signal?.aborted) throw error;
    throw new ServiceUnavailableError();
  });
  if (!response.ok) {
    const value = await response.json().catch(() => null);
    throw new Error(
      value?.error?.message ??
        `The local request failed (HTTP ${response.status}).`,
    );
  }
  return (await response.json()) as T;
}
export async function authenticate(): Promise<void> {
  const token = new URLSearchParams(location.hash.slice(1)).get("token");
  history.replaceState(null, "", location.pathname + location.search);
  if (token) {
    try {
      await api("/session", { token });
      return;
    } catch (error) {
      try {
        await api("/home");
        return;
      } catch {
        throw error;
      }
    }
  }
  await api("/home");
}

export const getWorkspace = () => api<Workspace>("/home");
export const getDatasource = (id: string) =>
  api<Datasource>(`/datasources/${encodeURIComponent(id)}`);
export const editDatasource = (id: string) =>
  api<SetupStatus>(`/datasources/${encodeURIComponent(id)}/edit`, {});
export const testDatasource = (id: string) =>
  api<ConnectionTest>(`/datasources/${encodeURIComponent(id)}/test`, {});
export const getSetup = (id: string) =>
  api<SetupView>(`/setups/${encodeURIComponent(id)}`);
export const getSetupStatus = (id: string) =>
  api<SetupStatus>(`/setups/${encodeURIComponent(id)}/status`);
export const saveSetup = (id: string, input: SetupSubmission) =>
  api<SetupStatus>(`/setups/${encodeURIComponent(id)}`, input);
export const cancelSetup = (id: string) =>
  api<SetupStatus>(`/setups/${encodeURIComponent(id)}/cancel`, {});
export const getResult = (id: string) =>
  api<ResultMetadata>(`/results/${encodeURIComponent(id)}`);
export const refreshResult = (id: string, requestId: string) =>
  api<ResultRefresh>(`/results/${encodeURIComponent(id)}/refresh`, {
    request_id: requestId,
  });
export const getRows = (
  id: string,
  statement: number,
  result: number,
  offset = 0,
  limit = 100,
) =>
  api<ResultPage>(
    `/results/${encodeURIComponent(id)}/rows?${new URLSearchParams({ statement: String(statement), result: String(result), offset: String(offset), limit: String(limit) })}`,
  );
export const cancelResult = (id: string) =>
  api<{ cancel_requested: boolean }>(
    `/results/${encodeURIComponent(id)}/cancel`,
    {},
  );

export const getPlugin = () => api<PluginManifest>("/plugin");
