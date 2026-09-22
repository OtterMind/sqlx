# @ottermind/sqlx-pi

Native SQLX tools for the Pi coding agent. The extension registers six `registerTool` tools and
calls the `sqlx` CLI for each one, so encrypted credentials, TLS policy, worker downloads and local
result pages stay CLI responsibilities.

## Install

```bash
pi install npm:@ottermind/sqlx-pi
```

Add `-l` to install into the project (`./.pi/settings.json`) instead of the user settings.

## Requirements

- Pi 0.86 or later.
- `@earendil-works/pi-coding-agent` and `typebox` are declared as peer dependencies; pi bundles
  both, and `pi install` runs npm with `--omit=peer`, so nothing extra is installed.
- The [`sqlx` CLI](https://www.npmjs.com/package/@ottermind/sqlx) 0.1.13 or later. The extension
  looks for `SQLX_BIN`, then `sqlx` on `PATH`, then the official user-level installation
  (`~/.local/bin/sqlx`, `%LOCALAPPDATA%\Programs\SQLX\sqlx.exe`, or `SQLX_INSTALL_DIR`), and when
  none of them exists it installs the CLI itself with
  `npx -y @ottermind/sqlx@latest --target pi`. Installing the package is therefore the only setup
  step.

## Tools

| Tool | Purpose | May modify data |
|---|---|---|
| `sqlx_datasource_list` | List saved datasources (no secrets) | no |
| `sqlx_datasource_show` | Show one datasource by UUID or unique name | no |
| `sqlx_datasource_test` | Test connectivity; executes no SQL | no |
| `sqlx_sql_execute` | Execute statements, return the table-shaped result with a preview (`full` returns every row) | yes |
| `sqlx_results_rows` | Read one page of a stored result; read-only | no |
| `sqlx_sql_view` | Execute once, return a local result-page URL | yes |
| `sqlx_prefetch` | Download `mysql`, `postgres`, `oracle`, `sqlserver`, `ui`, `skill` or `all` | downloads only |

The two execution tools describe the authorization rule themselves: run them only after the user
authorized that exact operation and scope. Results are never replayed automatically.

## Credentials

Save credentials in the SQLX data directory (`sqlx datasource add --connection-stdin`, or the local
`sqlx ui` page). A datasource created with `--username-env`/`--password-env` reads environment
variables at execution time, and a harness started from a GUI usually cannot see your shell
environment.

## Links

- Repository: <https://github.com/OtterMind/sqlx>
- Integration guide: <https://github.com/OtterMind/sqlx/blob/main/integrations/README.md>
