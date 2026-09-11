type Theme = "light" | "dark";
const storageKey = "sqlx.ui.theme";

export function initializeTheme(): void {
  const toggle = document.querySelector<HTMLButtonElement>("#theme-toggle")!;
  const system = window.matchMedia("(prefers-color-scheme: dark)");
  let preference: Theme | undefined;
  try {
    const saved = localStorage.getItem(storageKey);
    if (saved === "light" || saved === "dark") preference = saved;
  } catch {
    // Theme switching still works when browser storage is unavailable.
  }
  const apply = (theme: Theme) => {
    document.documentElement.dataset.theme = theme;
    const label = theme === "dark" ? "Light mode" : "Dark mode";
    toggle.setAttribute("aria-label", `Switch to ${label.toLowerCase()}`);
    toggle.title = `Switch to ${label.toLowerCase()}`;
  };
  const followSystem = () => apply(system.matches ? "dark" : "light");
  if (preference) apply(preference);
  else followSystem();
  system.addEventListener("change", () => {
    if (!preference) followSystem();
  });
  toggle.addEventListener("click", () => {
    preference =
      document.documentElement.dataset.theme === "dark" ? "light" : "dark";
    apply(preference);
    try {
      localStorage.setItem(storageKey, preference);
    } catch {
      // Preserve the selected appearance for this page even without storage.
    }
  });
}
