import { refreshResult } from "../sdk/client";
import type { ResultMetadata } from "../sdk/types";
import { button, element, message } from "./components";

/** The browser owns the interval; the service owns each explicitly started execution. */
export function refreshControls(
  id: string,
  signal: AbortSignal,
  poll: () => Promise<void>,
) {
  const controls = element("div", "refresh-controls");
  const refresh = button("Refresh data", "button secondary");
  refresh.title = "Run all original SQL statements again";
  const label = element("label", "auto-refresh", "Auto refresh");
  const interval = element("select");
  interval.setAttribute("aria-label", "Auto refresh interval");
  for (const seconds of [0, 5, 10, 30, 60]) {
    const option = element("option", "", seconds ? `Every ${seconds}s` : "Off");
    option.value = String(seconds);
    interval.append(option);
  }
  label.append(interval);
  const updated = element("span", "muted");
  const feedback = element("p", "feedback refresh-feedback");
  feedback.setAttribute("role", "status");
  feedback.hidden = true;
  controls.append(refresh, label, updated);
  let latest: ResultMetadata;
  let pending = false;
  let unresolvedRequest: string | undefined;
  let observedCompletion: string | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  function clear() {
    if (timer !== undefined) clearTimeout(timer);
    timer = undefined;
  }
  function busy() {
    return (
      pending ||
      latest?.status === "queued" ||
      latest?.status === "running" ||
      latest?.refresh?.status === "running"
    );
  }
  function schedule() {
    if (
      signal.aborted ||
      document.hidden ||
      busy() ||
      timer !== undefined ||
      interval.value === "0"
    )
      return;
    timer = setTimeout(
      () => {
        timer = undefined;
        void execute();
      },
      Number(interval.value) * 1000,
    );
  }
  function stop(error: string) {
    clear();
    interval.value = "0";
    feedback.hidden = false;
    feedback.className = "feedback error refresh-feedback";
    feedback.textContent = `${error} Auto refresh is off. The previous result is preserved.`;
  }
  function update(meta: ResultMetadata) {
    latest = meta;
    if (meta.refresh?.request_id === unresolvedRequest)
      unresolvedRequest = undefined;
    refresh.disabled = busy();
    refresh.textContent =
      pending || meta.refresh?.status === "running"
        ? "Refreshing…"
        : "Refresh data";
    updated.textContent = `Updated ${new Date(meta.created_at * 1000 + meta.duration_ms).toLocaleTimeString()}`;
    if (
      meta.refresh &&
      meta.refresh.status !== "running" &&
      observedCompletion !== meta.refresh.request_id
    ) {
      observedCompletion = meta.refresh.request_id;
      if (meta.refresh.status !== "completed")
        stop(meta.refresh.error ?? "Refresh did not finish.");
    }
    if (busy()) clear();
    else schedule();
  }
  async function execute() {
    if (signal.aborted || busy()) return;
    clear();
    pending = true;
    feedback.hidden = true;
    update(latest);
    // Reuse an unacknowledged request ID: a lost response must not repeat a write.
    unresolvedRequest ??= crypto.randomUUID();
    try {
      await refreshResult(id, unresolvedRequest);
      if (!signal.aborted) await poll();
    } catch (error) {
      if (!signal.aborted) stop(message(error));
    } finally {
      pending = false;
      if (!signal.aborted) update(latest);
    }
  }
  refresh.onclick = () => void execute();
  interval.onchange = () => {
    clear();
    schedule();
  };
  const visibility = () => {
    clear();
    schedule();
  };
  document.addEventListener("visibilitychange", visibility);
  signal.addEventListener(
    "abort",
    () => {
      clear();
      document.removeEventListener("visibilitychange", visibility);
    },
    { once: true },
  );
  return { controls, feedback, update, stop };
}
