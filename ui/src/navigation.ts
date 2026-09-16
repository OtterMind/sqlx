/** Internal links retain the workbench shell; modified clicks keep normal browser behavior. */
export function navigate(path: string, state: unknown = null): void {
  history.pushState(state, "", path);
  window.dispatchEvent(new PopStateEvent("popstate"));
}

export function startNavigation(
  render: (url: URL, signal: AbortSignal) => Promise<void>,
): () => void {
  let current: AbortController;
  const visit = () => {
    current?.abort();
    current = new AbortController();
    void render(new URL(location.href), current.signal);
  };
  document.addEventListener("click", (event) => {
    if (
      event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.ctrlKey ||
      event.shiftKey ||
      event.altKey
    )
      return;
    const link =
      event.target instanceof Element
        ? event.target.closest<HTMLAnchorElement>("a[href]")
        : null;
    if (
      !link ||
      link.hasAttribute("download") ||
      (link.target && link.target !== "_self")
    )
      return;
    const url = new URL(link.href);
    if (
      url.origin !== location.origin ||
      url.hash ||
      url.search ||
      !/^\/(?:$|(?:setup|result|datasource)\/[^/]+$)/.test(url.pathname)
    )
      return;
    event.preventDefault();
    if (url.pathname === location.pathname) return;
    navigate(url.pathname);
  });
  window.addEventListener("popstate", visit);
  window.addEventListener("pageshow", (event) => {
    if (event.persisted) visit();
  });
  window.addEventListener("pagehide", () => current?.abort());
  visit();
  return visit;
}

/** Only browser polling is cancelled. Database execution belongs to the local service. */
export function pollLater(
  signal: AbortSignal,
  task: () => Promise<void>,
  delay = 800,
): void {
  if (signal.aborted) return;
  const abort = () => clearTimeout(timer);
  const timer = setTimeout(() => {
    signal.removeEventListener("abort", abort);
    if (!signal.aborted) void task();
  }, delay);
  signal.addEventListener("abort", abort, { once: true });
}
