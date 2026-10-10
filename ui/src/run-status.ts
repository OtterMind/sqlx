import { t } from "./i18n";
import { getCatalog } from "../sdk/analytics";
import type { Catalog, ChartRun } from "../sdk/analytics";

/** Polls a chart run until it settles; a failure keeps the previous snapshot intact. */
export async function waitRun(run: ChartRun, signal: AbortSignal): Promise<Catalog> {
  while (true) {
    signal.throwIfAborted();
    const catalog = await getCatalog(signal), current = catalog.runs.find(r => r.request_id === run.request_id);
    if (current?.status === "failed") throw new Error(current.error ?? t("result.dataset.missingSnapshot"));
    if (current?.status === "completed") return catalog;
    await new Promise<void>((resolve, reject) => {
      const abort = () => { clearTimeout(timer); reject(signal.reason); };
      const timer = setTimeout(() => { signal.removeEventListener("abort", abort); resolve(); }, 500);
      signal.addEventListener("abort", abort, { once: true });
    });
  }
}
