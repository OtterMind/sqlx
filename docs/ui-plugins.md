# Build a SQLX UI plugin

A UI plugin is a static browser application. It provides the workspace, credential form and query result pages. The local Rust service provides authentication, connection testing, encrypted storage, SQL execution and retained results. The default UI has no private frontend API.

Use React, Vue, Svelte, vanilla TypeScript, or another framework that builds static assets. SQLX does not launch a Node.js server or native plugin code. This contract is introduced in SQLX 0.1.1; the published 0.1.0 CLI does not support it.

## Start with the independent example

From a SQLX checkout, build the CLI/service and both interfaces:

```sh
npm --prefix ui ci
npm --prefix ui run build
cargo build --workspace --locked
```

On macOS/Linux, select the locally built backend and install the example:

```sh
export PATH="$PWD/target/debug:$PATH"
export SQLX_WORKER_DIR="$PWD/target/debug"
sqlx ui plugin install --path examples/terminal-ui/dist
sqlx ui plugin use terminal
sqlx ui
```

On Windows PowerShell, set `$env:Path = "$PWD\target\debug;$env:Path"` and `$env:SQLX_WORKER_DIR = "$PWD\target\debug"`, then run the same `sqlx` commands.

The [terminal example](../examples/terminal-ui/app.ts) has its own HTML, CSS and application code. It imports only the [browser SDK](../ui/sdk/client.ts), not default UI components. Its form shows prepared settings, accepts credentials, supports saved-password handling and cancellation, and displays complete paginated query values. The richer [default plugin](../ui/src/) also offers editable settings, cell inspection and copying.

To develop a separate repository, copy the SDK's `client.ts` and `types.ts` into your source, preserve applicable license notices, and bundle them with your frontend. Change the plugin ID and name in `ui-plugin.json`. Bump the version after changing an installed build; installed versions are immutable. Rebuild, install, select, then reload the page. For Vite, set `base: "./"` and retain the HTML base placeholder below. Use production assets; development-server URLs are not supported.

## Package layout and manifest

```text
ui-plugin.json
index.html
app.js
app.css
LICENSE
NOTICE
assets/...
```

The manifest must be at the package root:

```json
{
  "schema_version": 1,
  "id": "my-interface",
  "name": "My SQLX Interface",
  "version": "0.1.0",
  "api_version": 1,
  "cli_compat": ">=0.1.1, <0.2.0",
  "entrypoint": "index.html",
  "capabilities": ["workspace", "datasource-setup", "query-results"],
  "description": "A short explanation of the interface."
}
```

IDs contain 1–64 lowercase letters, digits or hyphens. Versions use SemVer. `schema_version` and `api_version` must both be `1`; `cli_compat` must include the installed CLI. All three capabilities are required so every CLI-generated link works in the selected interface. The fields declare compatibility; authors must verify the actual behavior.

Place this in the entry HTML before relative asset references:

```html
<base href="__SQLX_UI_BASE__/" />
<link rel="stylesheet" href="app.css" />
<script type="module" src="app.js"></script>
```

SQLX substitutes a versioned base such as `/_ui/my-interface/0.1.0`. Use relative URLs for bundled assets, and absolute paths such as `/api/home` or `/result/<id>` for service requests and page navigation. Installed old versions retain their asset URLs when another version is selected.

Packages may contain HTML, JS/MJS, CSS, JSON, source maps, SVG/PNG/JPEG/WebP/GIF/ICO images, WOFF/WOFF2/TTF fonts, TXT/Markdown and LICENSE/NOTICE files. Hidden files, symlinks, path traversal and native executables are rejected. The entrypoint must be an HTML file containing the base placeholder. Ship built assets only, without `node_modules`, credentials, local configuration or build caches.

## Page routes and authentication

| Route | Required behavior |
|---|---|
| `/` | Show saved datasources, pending setup requests and retained query results. |
| `/datasource/<datasource-id>` | Show saved connection details, test connectivity and open its edit form. |
| `/setup/<request-id>` | Load prepared settings, collect credentials, show save errors/status and offer cancellation. |
| `/result/<result-id>` | Read execution status, SQL, metadata and rows. Show multiple result sets and partial failure. |

The current development service establishes an HttpOnly session cookie when it serves a local page. Call `await authenticate()` from the SDK to check workspace access before loading data. If the session expires, reload the page to establish another session. Do not log or persist cookies, passwords or setup submissions. API calls use same-origin cookies and the `X-SQLX-UI: 1` header; the SDK supplies both. No CORS or cross-origin development proxy is supported. From 0.1.3, authenticated requests renew the server session and HttpOnly cookie for 12 hours. A page heartbeat every 30 seconds keeps an open UI active; a page reload renews an expired session without a URL token.

## Browser API v1

All endpoints below are rooted at `/api`. Successful bodies are plain JSON objects, without the CLI's `data` wrapper. Domain errors return `{"error":{"message":"..."}}` with a non-2xx status. Malformed HTTP bodies may instead receive a framework error response. The [TypeScript types](../ui/sdk/types.ts) define the DTOs used by the SDK and both interfaces.

| Method and path | SDK helper | Purpose and response |
|---|---|---|
| `GET /home` | `getWorkspace()` | `{datasources, setups, results}`. Datasources include IDs, names and nonsecret connection settings; results include Unix `created_at` seconds. |
| `GET /datasources/<id>` | `getDatasource(id)` | Saved name and connection settings. Username, password and vendor properties are omitted. |
| `POST /datasources/<id>/test` | `testDatasource(id)` | Test with the saved credentials; return `{connected: true, duration_ms}` or an error. Body: `{}`. Does not save changes or execute SQL. |
| `POST /datasources/<id>/edit` | `editDatasource(id)` | Create a setup request from the saved connection and return `SetupStatus`. Body: `{}`. Navigate to `/setup/<request_id>`; this operation does not save changes. |
| `GET /plugin` | `getPlugin()` | Manifest of the selected UI. |
| `GET /setups/<id>` | `getSetup(id)` | Prepared name, connection fields, editing flag and current status. Saved passwords and vendor properties are omitted. |
| `GET /setups/<id>/status` | `getSetupStatus(id)` | Request ID, status, error and saved datasource ID, if completed. |
| `POST /setups/<id>` | `saveSetup(id, input)` | Submit `SetupSubmission`; returns status while the server tests and saves the connection asynchronously. |
| `POST /setups/<id>/cancel` | `cancelSetup(id)` | Cancel a waiting request without saving. Body: `{}`. |
| `GET /results/<id>` | `getResult(id)` | SQL statements, status, duration, tables and non-row execution events. |
| `POST /results/<id>/refresh` | `refreshResult(id, requestId)` | Rerun the saved SQL batch. Returns `{request_id, status, error}`; inspect metadata until completion. |
| `GET /results/<id>/rows?statement=0&result=0&offset=0&limit=100` | `getRows(id, statement, result, offset, limit)` | `{rows, offset, next_offset, total_rows, complete}`. |
| `POST /results/<id>/cancel` | `cancelResult(id)` | Request worker cancellation. Body: `{}`. Check result status afterward. |

Use the SDK's generic `api<T>(path, body?)` only when a typed helper is insufficient. API v1 additions must remain backwards compatible; breaking browser contract changes require a new API version. The plugin's CLI compatibility range can be narrower than the API's lifetime. Reject incompatible plugins rather than silently falling back to another interface.

The saved-datasource routes and `datasources` workspace field were added in 0.1.3. Existing plugins can continue using setup/result APIs without implementing datasource browsing. Saved connection links use stable datasource IDs, never names or result IDs. Listing, viewing and testing do not create a setup request or rewrite stored credentials; only an explicit edit action creates a form, and only a successful save persists changes.

### Credential forms

`SetupSubmission` contains `name`, `connection` and `password_action`. Copy prepared connection fields, accept the username and password in form inputs, and submit directly to the local service. Actions are `replace`, `clear` (explicit empty password), and `keep` (editing only; the server retrieves the existing secret). The service preserves vendor properties internally. Clear the password input after submission and never send it to an agent.

Status progresses from `waiting_for_user` to `saving`, then `completed`, or back to `waiting_for_user` with an error. A waiting request can become `cancelled` or `expired`. Disable duplicate submission while saving; poll status until the test finishes. The server rejects stale edits and saves only after the connection test succeeds. Setup requests expire after 30 minutes and do not survive service restarts.

### Query results

SQL runs once when the CLI submits `sql execute --view`. The browser can explicitly rerun an existing result through `POST /results/<id>/refresh` with `{request_id: <new UUID>}`. The service executes the saved statements exactly as supplied, including any writes. This endpoint cannot accept replacement SQL. The same request ID is idempotent, including after a newer refresh, and concurrent refreshes of the same result are rejected. Poll result metadata until `refresh.status` is terminal. Browser reload, paging, plugin switching and page reopening only read the current snapshot; they never initiate refresh. Browser endpoints cannot install/select plugins. Do not turn errors, empty results, expiry or an interrupted state into a new execution.

Statement and result indexes are zero-based. Rows are positional arrays so duplicate column labels remain separate. SQL NULL is JSON `null`; integer/decimal values are strings, and binary values use Base64 according to column encoding. Do not coerce exact strings to JavaScript numbers. Render values, SQL and errors as text, never HTML.

Limits are 1–200 rows (default 100); pages may contain fewer rows to keep responses near 2 MiB, while one large row remains complete. Advance using `next_offset`, not the requested limit; store prior offsets for backwards paging when rows are large. `total_rows` can grow during execution. A result may include several tables and an error/skipped statement event. Display update counts from `affected_rows` as well as row data.

Poll queued/running results without overlapping requests, stop polling at a terminal state, and clean up timers when leaving a route. Cancellation can leave an unknown write outcome; never infer rollback. Results survive restarts for 24 hours; unfinished runs recover as interrupted without replay.

## Publish and install

Publish a ZIP of the built directory's **contents**, with `ui-plugin.json` at its root, to your own GitHub Release. Publish the SHA-256 alongside it. No change to SQLX's release or backend is required for community plugins.

```sh
sqlx ui plugin install --url https://github.com/OWNER/REPO/releases/download/v0.1.0/my-interface.zip --sha256 <sha256>
sqlx ui plugin use my-interface --version 0.1.0
```

`install --path <directory>` supports local builds. Installation does not select the plugin. `use` without a version selects the highest installed compatible version. Stop the UI before removing inactive versions so open pages keep their assets. SQLX stores plugins in `~/.sqlx/plugins/ui/` (or the configured data directory); per-file hashes are verified at installation and asset reads.

Plugins are trusted code in the credential/result page and can access its inputs and data. The CSP restricts resources and requests to the local origin, blocks inline scripts/styles and embedding, and allows data images. Bundle dependencies locally and use external CSS. This is not a sandbox that isolates a malicious interface from credentials entered into it. A SHA-256 verifies bytes, not author identity. The backend exposes no key retrieval or native hooks. Browser refresh can rerun a saved SQL batch, but the browser cannot submit replacement SQL through that endpoint.

## Acceptance checklist

Verify all three routes with real CLI-generated links; wrong-password correction, cancellation, edit/keep-password and expired sessions; multiple result sets, exact numbers, NULL/empty values, large cells and pages; reloads and UI switching without SQL replay; clean console/network behavior under SQLX's CSP. Use a dedicated test datasource. `tests/ui_plugins.py` and `tests/ui_api.py` exercise the shared service contract; a plugin author must also test their interface in a browser.

The result metadata fields `snapshot` and `refresh` were added in 0.1.3. `refresh` contains the latest request ID, status (`running`, `completed`, `failed`, `cancelled`, `interrupted`) and error. The previous snapshot stays readable during refresh. Successful completion changes `snapshot` atomically; reset page offsets when it changes. An optional `snapshot=<id>` row-query parameter (`initial` for the first execution) rejects reads against a replaced snapshot. Failed refreshes do not discard prior rows or roll back database changes. Optional timed refresh belongs to the browser: disable by default, avoid overlap, pause in hidden tabs, stop on error, and dispose it on navigation.
