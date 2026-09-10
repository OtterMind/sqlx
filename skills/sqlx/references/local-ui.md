# Local credential and result pages

These commands require SQLX 0.2.0. The page is served by a separately downloaded local service and the selected UI plugin, and opens on the machine where the CLI runs. A local URL from a remote SSH environment does not automatically open the remote page on the user's computer.

## Ask the user for a password without putting it in the conversation

**Purpose:** Prepare all known connection parameters and let the user fill the remaining credentials directly in a local browser form.

```sh
sqlx datasource add --ui --name dev --type postgresql --host db.example.com --port 5432 --database app
```

**Replace:** The name, database type, host, port, and database with the intended connection. For Oracle, use `--service <service-name>`. `--username-env <variable>` can prefill an already available account name. Do not pass `--password-env` or `--connection-stdin` with `--ui`.

**Result:** The CLI returns a local URL, `request_id`, and `status: waiting_for_user`. It normally opens the page; add `--no-open` to return the link only. No datasource is saved until the user chooses **Save & connect** and the connection test succeeds.

The form pre-fills connection settings in an expandable section. The user can change them, enter username/password, and save. Connection errors remain on the page so the user can correct them. Cancellation does not save a datasource.

## Resume after the user saves

**Purpose:** Learn whether a browser setup completed without retrieving credentials.

```sh
sqlx datasource setup-status --request-id <request-id>
```

**Result:** `waiting_for_user`, `saving`, `completed`, `cancelled`, or `expired`, plus a datasource ID when saved. Use that ID in the subsequent SQL command. Wait for the user's action rather than repeatedly polling during a long manual entry. A setup request expires after 30 minutes and is lost when the UI service stops; create a new one if needed.

## Update credentials for an existing connection

**Purpose:** Open the existing connection for human editing.

```sh
sqlx datasource update --id <datasource-id> --ui
```

**Result:** A prefilled edit page with no saved password returned to the browser. **Keep saved password** retains the existing secret; **Enter a new password** replaces it; **Use an empty password** is an explicit separate choice. If another CLI call changes the datasource while the form is open, saving is rejected instead of overwriting the concurrent change.

## Open a query result page

**Purpose:** Show the result of SQL already prepared by the agent.

```sh
sqlx sql execute --datasource <datasource-id> --sql "SELECT id, name FROM users ORDER BY id" --view
```

**Replace:** The datasource ID and query. Repeat `--sql` for multiple complete statements in one connection, following normal CLI execution semantics.

**Result:** A local URL and `result_id`. Execution starts in the UI service once; the page automatically shows progress and then results. It includes per-statement/result-set tabs, exact values, pagination, full cell inspection, and errors with skipped statements. Browsing, refreshing, and reopening do not rerun SQL. Requesting a new execution through the CLI is an explicit new operation.

Rows remain complete in the local cache, while the browser loads only a page at a time. Cached results expire after 24 hours. Completed results can be viewed after restarting the UI service; unfinished results recover as interrupted and are never automatically replayed. A cancelled or disconnected write can still have an unknown database outcome.

## Manage the companion

```sh
sqlx ui
sqlx ui status
sqlx ui stop
```

**Purpose and result:** The first command opens recent local requests/results, the second reports whether the service is running, and the third stops it and cancels active work. The service also exits after 30 idle minutes when no task is active. Closing a browser tab does not cancel a query.

Launch links contain a short-lived one-use token in the URL fragment; the page exchanges it for a local browser session. Do not publish those links or credentials externally. The service validates local authentication and request origins. Credential entry avoids sending the password through normal agent arguments and responses, but it does not isolate secrets from an agent with the same user's file or browser access.

## Select a UI plugin

**Purpose:** Use an interface the user has chosen while keeping saved datasources and cached executions.

```sh
sqlx ui plugin install --url <plugin-zip-url> --sha256 <published-sha256>
sqlx ui plugin list
sqlx ui plugin use <plugin-id>
```

**Replace:** Use the author's release ZIP URL, independently obtained SHA-256, and installed plugin ID. For a local build, use `sqlx ui plugin install --path <built-directory>` instead. API/CLI compatibility and content integrity are checked. Installation does not change the active interface. `use` optionally accepts `--version <version>`; otherwise it selects the highest installed compatible version. Reload the page after switching. Existing result IDs and setup requests are preserved, and SQL is not rerun.

The first UI launch automatically installs and selects the default interface if none is selected. Return to it with `sqlx ui plugin use default` once installed. A plugin can read entered credentials and browser-visible results, so install one only when its source is trusted by the user. A checksum does not establish author trust. Do not replace the user's selected UI as a side effect of ordinary database work.

To remove an inactive version, run `sqlx ui stop`, then `sqlx ui plugin remove <plugin-id> --version <version>`. Stop cancels active work and loses unfinished setup requests, so do not stop the service casually just to remove an old plugin. Contributor documentation: [UI plugin guide](https://github.com/OtterMind/sqlx/blob/main/docs/ui-plugins.md).
