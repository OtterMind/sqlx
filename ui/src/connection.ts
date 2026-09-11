import { api } from "./api";
import { button, element, message } from "./components";

/** An open workspace is activity, even when its cached result no longer needs polling. */
export function keepConnectionAlive(retryPage: () => void): void {
  const notice = element("div", "connection-notice");
  const text = element("span");
  const retry = button("Retry", "button secondary");
  notice.setAttribute("role", "status");
  notice.hidden = true;
  notice.append(text, retry);
  const toggle = document.getElementById("theme-toggle")!;
  toggle.before(notice);
  let pending: Promise<void> | undefined;
  const check = () => {
    if (pending) return pending;
    pending = api("/home", undefined, AbortSignal.timeout(5000))
      .then(() => {
        notice.hidden = true;
      })
      .catch((error: unknown) => {
        text.textContent = "SQLX connection lost";
        notice.title = message(error);
        notice.hidden = false;
        throw error;
      })
      .finally(() => {
        pending = undefined;
      });
    return pending;
  };
  retry.onclick = async () => {
    retry.disabled = true;
    try {
      await check();
      retryPage();
    } catch {
      /* The connection notice stays visible. */
    } finally {
      retry.disabled = false;
    }
  };
  // A bounded read keeps the existing server alive; it never executes or retries SQL.
  window.setInterval(() => {
    void check().catch(() => {});
  }, 30_000);
  window.addEventListener("online", () => {
    void check().catch(() => {});
  });
}
