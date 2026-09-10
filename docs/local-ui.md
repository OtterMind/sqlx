# Local UI implementation

The CLI launches a separately downloadable `sqlx-ui` process on loopback. UI assets are built from TypeScript and embedded in that executable. `sqlx_core` shares encrypted storage, component management and worker execution with the regular CLI; the main binary does not link Axum or database drivers.

## Commands

- `datasource add --ui ...` creates a prefilled setup request without prompting for or accepting a password in the command. `--username-env` can prefill a known username.
- `datasource update --id <id> --ui` opens an edit form. Keeping the existing password, replacing it, and explicitly using an empty password are distinct actions.
- `datasource setup-status --request-id <id>` returns status and a saved datasource ID, never the password.
- `sql execute --datasource <id> --sql "..." --view` starts one execution in the UI service and returns its result URL. A repeated internal request ID is idempotent for the same datasource/SQL.
- `ui`, `ui status`, and `ui stop` manage the local companion. `--no-open` prints a launch link without starting a browser.

## Credentials and local authentication

The server binds an OS-selected port on `127.0.0.1`. A private state file contains its instance ID and CLI control token. The CLI verifies the authenticated instance before reusing it. Browser launch links carry a one-use, five-minute bootstrap token in the fragment. The page removes the fragment and exchanges it for an HttpOnly, SameSite=Strict cookie. Cookies are named per server instance so separate SQLX services do not overwrite each other's sessions; requests still require the instance's authenticated API and origin checks.

The server validates Host and Origin, requires a custom header for browser API calls, and separates CLI-only control endpoints from browser endpoints. Pages and scripts are served locally with a restrictive CSP; SQL, errors, and result values are rendered as text. No password is placed in URL parameters, browser storage, request logs, status responses, or persisted setup drafts.

A setup request lives in server memory for up to 30 minutes. Save first checks the database connection, then writes encrypted storage. Editing uses a fingerprint of the original datasource to reject stale form writes. Cancel and expiry do not write a datasource. Credentials still share the operating-system user's trust boundary with the CLI and local key file.

## Result lifetime

Worker events go through the same validator as stdout execution. UI mode writes row JSONL plus fixed-width offset indexes to owner-restricted files under `results/<id>/`. Metadata records statement/result boundaries and exact column types. Pages read a bounded row window without repeating SQL or scanning preceding rows. Large cells remain complete and can be opened in a value dialog.

Completed results survive UI service restarts for 24 hours. Unconfirmed active results recover as interrupted; they are never replayed. Refresh, pagination, and reopening a link are read operations. The service exits after 30 idle minutes with no active task. Explicit shutdown cancels active workers and preserves their outcome state.

## Validation

Build UI assets with `npm --prefix ui ci` and `npm --prefix ui run build` before workspace Rust commands. `tests/ui_lifecycle.py` exercises detached start/reuse/stop on all supported platforms. `tests/ui_api.py` uses the isolated PostgreSQL fixture to cover browser authentication, credential saving and edit conflicts, pagination without re-execution, restart recovery, cancellation, and first-error behavior. Interactive browser acceptance uses Playwright CLI.
