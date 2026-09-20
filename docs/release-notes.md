SQLX 0.1.7 adds Skill targets for dsh and Pi, a way to remove a Skill installation, and clearer database errors.

## New

- `sqlx skill install --target dsh` installs the Skill into `~/.agents/skills`, the shared directory that dsh and Codex both read; `--target pi` installs into Pi's `~/.pi/agent/skills`. The npx installer accepts the same names, and unknown targets now list all four.
- `sqlx skill remove --path <skill-directory>` stops managing an installation without deleting its files, and `sqlx skill status` marks a record whose directory no longer exists with `missing: true` instead of leaving an unexplained `intact: false`.

## Improvements

- PostgreSQL failures report the server's own message instead of `db error`, including the detail and hint fields when the server sends them.
- A connection that fails before any statement with certificate verification enabled now points at `--tls disable`. Oracle previously reported only `ORA-17002: I/O 错误: Connection closed`, which looks like a network fault rather than a TLS mismatch.
- The JDBC worker no longer writes vendor driver diagnostics to stderr when a connection fails; set `SQLX_JDBC_DEBUG=1` to see them again.
- Credential redaction keeps hostnames and ordinary words intact. A password such as `oracle` no longer rewrites `docs.oracle.com` in a driver message, and a short username no longer mangles unrelated words.

## Fixes

- `sqlx sql execute … | head` no longer panics with `failed printing to stdout: Broken pipe`; a closed stdout exits quietly and other write failures still fail.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin. This release does not change the local page plugin, so no page update is required. Downloaded components are cached per version and reused, so an upgrade only fetches components that changed. Update managed Skills separately with `sqlx skill update` after upgrading the CLI; the 0.1.7 Skill requires SQLX 0.1.7.

The Skill's approval rule constrains the Agent workflow only. SQLX itself still executes the supplied SQL unchanged, and an explicit result refresh still reruns the complete original batch, including any writes.

Prebuilt packages are available for macOS ARM64/x64, Linux ARM64/x64, and Windows x64. macOS executables are Developer ID signed and notarized. See LICENSE and NOTICE for license conditions.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.6...v0.1.7
