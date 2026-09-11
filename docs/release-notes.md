SQLX 0.1.3 adds saved datasources to the workbench. Open a connection to inspect its settings, test connectivity or edit it while retaining its saved password. Pending connection requests and query history remain available separately.

Result pages now offer Refresh and optional 5/10/30/60-second automatic refresh. Each action executes the original SQL batch unchanged and in order. Successful refreshes replace the snapshot at the same URL; failure or cancellation preserves the previous result. Automatic refresh waits for completion, pauses in hidden tabs and stops on errors or navigation. Browser reload and pagination still read cached data. Writes in an explicitly refreshed batch execute again.

Active pages renew their browser session automatically. The five-minute launch link only limits initial authorization. Saved dark/light preferences are restored before the first styled paint, avoiding a light flash during dark-mode reloads.

The Skill now requires agents to include the complete, clickable page URL in their response, including its authorization fragment, even when a browser was opened automatically.

Upgrade SQLX 0.1.2 with `sqlx update check`, `sqlx update install` and `sqlx update status`. Update managed Skills separately with `sqlx skill update`. The updater preserves running UI processes and selected plugins; restart the UI service and explicitly install/select the 0.1.3 default plugin to use its new page features. Versions 0.1.0/0.1.1 need the README installer first.

Prebuilt packages cover macOS ARM64/x64, Linux ARM64/x64 and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.
