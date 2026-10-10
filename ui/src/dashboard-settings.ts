import type { Dashboard } from "../sdk/analytics";
import { promptDialog } from "./components";
import { t } from "./i18n";

/** Renames a dashboard and edits its description; the live dashboard autosaves the result. */
export function dashboardSettings(current: Dashboard, signal: AbortSignal, apply: (value: Dashboard) => void): void {
  void promptDialog({
    title: t("settings.title"), submitLabel: t("settings.apply"), signal,
    fields: [
      { label: t("settings.field.name"), id: "board-name", value: current.name, required: true },
      { label: t("settings.field.description"), id: "board-description", value: current.description },
    ],
  }).then(answer => {
    if (answer) apply({ ...current, name: answer["board-name"], description: answer["board-description"] });
  });
}
