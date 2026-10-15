SQLX 0.1.21 makes every integration deliver the Skill, runs the browser connection setup through tools, and lets a client with no plugin system start the server in one line.

## A running call can be stopped

- `notifications/cancelled` now reaches the call it names and stops the worker behind it; the call answers with `success: false`, `worker.incomplete` and the warning that an in-flight write may have completed. The server read stdin on the same thread that ran the call, so the notification used to wait behind the very statement it was meant to interrupt.
- Registrations are keyed by request id, so several calls stay cancellable on their own and a late cancellation belongs to nobody.
- A client that sends a `progressToken` receives `notifications/progress` for every statement and for every component `sqlx_prefetch` downloads, so a first query waiting on a download is visibly working instead of apparently hung.
- `initialize` returns `instructions`, so a client that installed no Skill still learns the credential and authorization rules.

## Adding a connection is a tool

- `sqlx_datasource_setup` starts the browser page where the user types the credentials and returns that page's URL; `sqlx_datasource_setup_status` reports the outcome. Neither carries a credential, and the dsh and pi packages register both, so all four integrations offer the same nine tools.
- The flow used to require a shell to run `sqlx datasource add --ui`, so a harness without one could not reach it at all.
- A lookup that finds no datasource now names the next step instead of answering `datasource not found`.

## Every integration installs the Skill

- The CLI being present is the common case — it is the first step of the Quick start — and every integration only ran the installer when the CLI was missing. On those machines the Skill was never installed: the agent had the tools but none of the 31 engine recipes, and nothing said so. Each integration now checks its own Skill directory on first use and installs the Skill in the background, with stdout kept off the MCP protocol stream.
- `scripts/install.sh`, `scripts/install.ps1` and the npm installer report where the Skill goes and which command puts it where an agent looks.

## A client with no plugin system

- The npm package runs a SQLX command, so an MCP client is configured with one line: `npx -y @ottermind/sqlx@latest mcp` starts the server and installs the CLI on first use. Without a command the package still installs, which is how it has always been invoked.
- `--cli-only` installs or updates the CLI without the Skill.

## Smaller corrections

- The dsh and pi shims keep the structured result when a statement fails, instead of replacing `results[].error`, `outcome` and `skipped` with the process error; `datasource list` and `datasource show` no longer unwrap an error envelope into `undefined`.
- The dsh and pi shims require the CLI release they were verified against and refuse an explicit `SQLX_BIN` that is older, instead of quietly running a different executable.
- pi declares the same MCP tool annotations as the server, which a permission extension uses to decide what to confirm. dsh has no such field, and its README says so rather than implying the harness enforces the rule.
- `sqlx update install` lists managed Skills whose `cli-compat` range excludes the installed CLI, in `outdated_skills`.
- The prefetch component list no longer enumerates a stale subset in the two shims or their READMEs.

## Upgrading

Run `sqlx update check` and `sqlx update install`. The 0.1.21 Skill is published with a `>=0.1.21, <0.2.0` CLI requirement, so update the CLI before the Skill; use `sqlx skill update` for managed installations.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.20...v0.1.21

SQLX 0.1.20 documents the analytics workspace and corrects the TLS default in the README and the Skill.

## Charts and dashboards are documented

- The command table lists `sqlx analytics chart add`, `chart run`, `dashboard add`, `list` and `remove`, and the local-pages section points at them.
- The Skill ships a new reference, `references/analytics.md`: what a chart owns (its read-only SQL, its datasource and its snapshot), the thirteen chart types and which axes each one needs, the read-only checks, and where snapshots live. The Skill description and its routing table now mention charts and dashboards, so an agent finds the feature without reading `--help`.

## The TLS default is stated correctly

- The README and `references/connections.md` both claimed a connection verifies the database certificate by default. The shipped default is `disable`: `--tls verify-full` is the opt-in, and it fails with `invalid peer certificate: UnknownIssuer` against a database that serves no trusted certificate, which is the common case for a local container. Both files now say so.

## Smaller corrections

- The import response's `total` is explained: it counts the stored connections plus the document's entries, including the ones reported in `skipped`.
- `references/cli.md` records that installing the Skill into an existing directory SQLX does not manage fails with `skill destination already exists and is not managed by SQLX` instead of overwriting another tool's files.
- `sqlx analytics chart add` takes `--command`, matching `sqlx sql execute`; `--sql` still works as an alias, so existing scripts and examples keep running.

## Upgrading

Run `sqlx update check` and `sqlx update install`. The 0.1.20 Skill is published with a `>=0.1.20, <0.2.0` CLI requirement, so update the CLI before the Skill; use `sqlx skill update` for managed installations.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.19...v0.1.20
