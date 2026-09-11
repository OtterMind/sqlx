SQLX 0.1.2 adds `update check`, `update install` and `update status`. The CLI checks official stable releases, verifies downloaded and installed executables, and preserves the previous binary for recovery when replacement fails. Interactive use can check once daily in the background; updates are installed only when requested. Existing connections, running SQL, UI processes and chosen plugins are preserved.

The default UI now uses a compact workbench with connection requests and query history, a collapsed SQL preview, dense result tables and a persistent light/dark icon switch. Navigation changes the content pane without reloading the whole page. Open pages keep the local service active, and connection failures provide a retry action without replaying SQL.

For 0.1.0 or 0.1.1, rerun the README installer once to install 0.1.2, then use the new update commands. Skills and UI plugins retain their separate installation/update controls. MySQL/PostgreSQL workers and private JDBC dependencies still download only when needed from the matching release manifest.

Prebuilt packages cover macOS ARM64/x64, Linux ARM64/x64 and Windows x64. macOS executables are Developer ID signed and notarized. This release does not include unattended update installation, telemetry, SQL-file input or persistent cross-call database sessions. See LICENSE and NOTICE for the project's license conditions.
