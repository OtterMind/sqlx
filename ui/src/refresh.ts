import { refreshResult } from "../sdk/client";
import type { ResultMetadata } from "../sdk/types";
import { button, choiceMenu, element, message, svgIcon } from "./components";

/** The browser owns the interval; the service owns each explicitly started execution. */
export function refreshControls(
  id: string,
  signal: AbortSignal,
  poll: () => Promise<void>,
) {
  const controls = element("div", "refresh-controls");
  const group = element("div", "refresh-button");
  const refresh = button("", "button secondary refresh-main");
  const caption = element("span");
  refresh.append(svgIcon("M20 11a8 8 0 1 0-2.3 6 M20 4v7h-7"), caption);
  refresh.setAttribute("aria-label", "Refresh data");
  const interval = choiceMenu(
    "Auto refresh interval",
    [0, 5, 10, 30, 60].map((seconds) => [
      String(seconds),
      seconds ? `Every ${seconds}s` : "Manual refresh",
    ]),
    () => {
      clear();
      renderButton();
      schedule();
    },
    signal,
  );
  interval.element.classList.add("refresh-options");
  group.append(refresh, interval.element);
  const updated = element("span", "muted");
  const feedback = element("p", "feedback refresh-feedback");
  feedback.setAttribute("role", "status");
  feedback.hidden = true;
  controls.append(group, updated);
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
  function renderButton() {
    refresh.disabled = busy();
    refresh.setAttribute(
      "aria-busy",
      String(pending || latest?.refresh?.status === "running"),
    );
    const automatic = interval.value !== "0";
    group.classList.toggle("auto-refresh", automatic);
    caption.hidden = !automatic;
    caption.textContent = automatic ? `${interval.value}s` : "";
    refresh.title = automatic
      ? `Refresh now · Auto refresh every ${interval.value}s`
      : "Refresh now";
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
  function stop(error: string, previous = false) {
    clear();
    interval.value = "0";
    renderButton();
    feedback.hidden = false;
    feedback.className = "feedback error refresh-feedback";
    feedback.textContent = `${previous ? "Previous refresh failed" : "Refresh failed"}: ${error} Auto refresh is off. Showing the last successful result.`;
  }
  function update(meta: ResultMetadata) {
    const previous = latest === undefined;
    latest = meta;
    if (meta.refresh?.request_id === unresolvedRequest)
      unresolvedRequest = undefined;
    updated.textContent = `Updated ${new Date(meta.created_at * 1000 + meta.duration_ms).toLocaleTimeString()}`;
    if (
      meta.refresh &&
      meta.refresh.status !== "running" &&
      observedCompletion !== meta.refresh.request_id
    ) {
      observedCompletion = meta.refresh.request_id;
      if (meta.refresh.status === "completed") feedback.hidden = true;
      else stop(meta.refresh.error ?? "Refresh did not finish.", previous);
    }
    renderButton();
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
