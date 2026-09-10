export async function api<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(`/api${path}`, {
    method: body === undefined ? "GET" : "POST",
    headers: {
      "X-SQLX-UI": "1",
      ...(body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    credentials: "same-origin",
    body: body === undefined ? undefined : JSON.stringify(body),
    cache: "no-store",
  });
  const value = await response.json();
  if (!response.ok)
    throw new Error(value.error?.message ?? "The local request failed.");
  return value as T;
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
