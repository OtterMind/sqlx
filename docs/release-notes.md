SQLX 0.2.0 adds local browser pages for connection setup and query results. Agents prefill connection settings with `datasource add --ui`; users enter passwords directly in the browser, and the CLI returns only setup status and the saved datasource ID.

Queries submitted with `sql execute --view` run once and open a page with progress, result-set tabs, exact values and pagination. Refreshing or reopening the page reads cached results without rerunning SQL. The UI component downloads separately; `ui status` and `ui stop` manage its lifecycle.

Existing CLI JSON execution and encrypted datasources remain compatible. macOS executables are Developer ID signed and notarized in the release workflow; Windows and Linux artifacts are separate platform builds. Java remains a separately downloaded vendor runtime. UI assets are bundled and require no user-installed Node.js.

The license contains additional conditions beyond standard Apache 2.0; see LICENSE and NOTICE. Cross-call database sessions, SQL-file input, configurable transaction modes, automatic CLI updates, and telemetry are not included. Browser password entry avoids normal model-context exposure; it does not isolate secrets from processes with the same operating-system user's access.
