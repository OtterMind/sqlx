import { activateLocale, detectLocale } from "./i18n/core";
import { restoreTheme } from "./theme";

// A small blocking head script restores the saved theme and language before anything paints.
restoreTheme();
activateLocale(detectLocale());
