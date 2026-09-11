# Local UI implementation

The CLI launches a separately downloadable `sqlx-ui` process on loopback. Frontend assets are independently installed static UI plugins; no interface is embedded in the executable. The default UI and the terminal-style example use the same browser API v1. `sqlx_core` shares encrypted storage, component management and worker execution with the regular CLI; the main binary does not link Axum or database drivers.

SQLX 0.1.3 adds saved datasources to the default workbench sidebar and home page. Selecting one shows its nonsecret settings and offers explicit connection testing and editing. Editing reuses the existing credential form and stale-edit protection; a successful save updates the sidebar. Completed/cancelled setup tasks no longer occupy the pending connection-request list.

## Commands

- `datasource add --ui ...` creates a prefilled setup request without prompting for or accepting a password in the command. `--username-env` can prefill a known username.
- `datasource update --id <id> --ui` opens an edit form. Keeping the existing password, replacing it, and explicitly using an empty password are distinct actions.
- `datasource setup-status --request-id <id>` returns status and a saved datasource ID, never the password.
- `sql execute --datasource <id> --sql "..." --view` starts one execution in the UI service and returns its result URL. A repeated internal request ID is idempotent for the same datasource/SQL.
- `ui`, `ui status`, and `ui stop` manage the local companion. `--no-open` prints a launch link without starting a browser.

## UI plugins

The default plugin downloads through the compatible release manifest on first use. A selected local or community plugin is used directly without downloading the default. Plugin installation, selection and removal use `sqlx ui plugin install`, `list`, `use` and `remove`. Built directories and HTTPS ZIP archives with explicit SHA-256 checksums are supported.

Plugins live under `plugins/ui/<id>/<version>/` with a file integrity receipt. `active.json` is the single selection source, updated atomically under a registry lock. HTML requests read that selection; old pages keep their versioned asset URLs. The service never restarts a query when selecting or loading an interface. Removing a plugin requires the UI service to be stopped and the version to be inactive.

The plugin owns the workspace, credential form and result display. Rust owns browser authentication, encrypted credentials, driver management, execution and retained results. Plugins receive no native hooks or filesystem API. A plugin is trusted page code and can access the credentials entered into its form and browser-visible results. See [the contributor guide](ui-plugins.md) for the manifest, SDK and API contract.

## Credentials and local authentication

The server binds an OS-selected port on `127.0.0.1`. A private state file contains its instance ID and CLI control token. The CLI verifies the authenticated instance before reusing it. Browser launch links carry a one-use, five-minute bootstrap token in the fragment. The page removes the fragment and exchanges it for an HttpOnly, SameSite=Strict cookie. Cookies are named per server instance so separate SQLX services do not overwrite each other's sessions; requests still require the instance's authenticated API and origin checks.

The server validates Host and Origin, requires a custom header for browser API calls, and separates CLI-only control endpoints from browser endpoints. Pages and scripts are served locally with a restrictive CSP; SQL, errors, and result values are rendered as text. No password is placed in URL parameters, browser storage, request logs, status responses, or persisted setup drafts.

A setup request lives in server memory for up to 30 minutes. Save first checks the database connection, then writes encrypted storage. Editing uses a fingerprint of the original datasource to reject stale form writes. Cancel and expiry do not write a datasource. Credentials still share the operating-system user's trust boundary with the CLI and local key file.

## Result lifetime

Worker events go through the same validator as stdout execution. UI mode writes row JSONL plus fixed-width offset indexes to owner-restricted files under `results/<id>/`. Metadata records statement/result boundaries and exact column types. Pages read a bounded row window without repeating SQL or scanning preceding rows. Large cells remain complete and can be opened in a value dialog.

Completed results survive UI service restarts for 24 hours. Unconfirmed active results recover as interrupted; they are never replayed. Browser reload, pagination and reopening a link read the current local snapshot. The explicit Refresh action reruns the original SQL batch without classification or rewriting. The arrow beside Refresh selects manual mode or a 5/10/30/60-second interval. The optional browser interval repeats that action after the prior run completes; it pauses in a hidden tab and is disabled on navigation, reload or failure. A complete refreshed snapshot atomically replaces the prior one at the same URL. Failure/cancellation keeps the prior result but does not undo database writes. Refresh request IDs have durable receipts so retrying a lost response cannot execute a batch twice. The service exits after 30 idle minutes with no active task. The default UI sends an authenticated read every 30 seconds while its page is open, so viewing a completed result is treated as activity. A lost connection shows a retry action; retries only reload page data and never rerun SQL. Authenticated API activity renews both the browser cookie and server session for 12 hours; the five-minute launch token is only for initial authorization. Explicit shutdown cancels active workers and preserves their outcome state.

## Validation

Build UI assets with `npm --prefix ui ci` and `npm --prefix ui run build` before workspace Rust commands. `tests/ui_lifecycle.py` exercises detached start/reuse/stop on all supported platforms. `tests/ui_api.py` uses the isolated PostgreSQL fixture to cover browser authentication, credential saving and edit conflicts, pagination without re-execution, restart recovery, cancellation, and first-error behavior. `tests/ui_plugins.py` covers local/ZIP installation, compatibility rejection, version immutability, live switching, asset integrity and removal guards. `tests/ui_distribution.py` verifies automatic download of both the service and default plugin. Interactive browser acceptance uses Playwright CLI for both interfaces.
