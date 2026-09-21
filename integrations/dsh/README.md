# @ottermind/dsh-sqlx

Native SQLX tools for DeepSeek Harness. The plugin registers six `defineTool` tools and calls the
`sqlx` CLI for each one, so encrypted credentials, TLS policy, worker downloads and local result
pages stay CLI responsibilities.

## Install

```bash
dsh plugin --profile <profile> add @ottermind/dsh-sqlx
```

`cordis.patch.yml` is declared as `dsh.bundle.patch`, so installing the package appends the plugin
row to the profile's bundle stack; no profile file edits are needed. Remove it with
`dsh plugin --profile <profile> remove @ottermind/dsh-sqlx`.

## Requirements

- DeepSeek Harness 0.1.5 or later.
- `@deepseek-ai/dsh-tools` (peer dependency) — bundled with dsh, and the profile installer runs
  pnpm with `autoInstallPeers: false`, so nothing extra is installed.
- The [`sqlx` CLI](https://www.npmjs.com/package/@ottermind/sqlx) 0.1.10 or later on `PATH`.
  Set `SQLX_BIN` to point at a specific executable instead.

## Tools

| Tool | Purpose | May modify data |
|---|---|---|
| `sqlx_datasource_list` | List saved datasources (no secrets) | no |
| `sqlx_datasource_show` | Show one datasource by UUID or unique name | no |
| `sqlx_datasource_test` | Test connectivity; executes no SQL | no |
| `sqlx_sql_execute` | Execute statements, return the full structured result | yes |
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
