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
