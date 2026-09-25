SQLX 0.1.17 connects to eight more databases, four of them with a driver you install once from the vendor.

## New engines

- Presto, Hive, Apache Kylin and XuguDB join the engines whose driver ships with the release. Their JDBC components download like Oracle's or SQL Server's, and each one carries what its driver needs: Presto and Kylin their own dependencies, Hive its standalone driver and logging binding.
- IBM Db2, IBM Informix, SUNDB and GBase 8s connect with a driver their vendor does not allow SQLX to redistribute. `sqlx driver add --type <engine> --jar <path>` installs the jar you obtained: it checks that the jar really carries the engine's driver class, stores it in `<data-dir>/drivers/<engine>/`, and every later command loads it. `sqlx driver list` shows where each engine's driver comes from and `sqlx driver remove --type <engine>` takes it back. Running a statement without a driver fails before anything is downloaded and names the command to run.
- A jar you provide is loaded before the released one it replaces, so a newer vendor driver works for any engine without waiting for a release.

## New in the JDBC worker

- A driver that does not implement `isValid` or `getMoreResults` no longer turns a working statement into an error: GBase 8s reports both as a plain `SQLException`, and only a driver that says the method is unsupported ends the call early.
- A driver that prints to standard output while answering a query no longer corrupts the event stream, which is how Kylin's Avatica client behaved; the CLI reports such a line on standard error, with credentials redacted, and keeps reading.
- The Informix-derived engines receive their server instance and client locale in the URL, and the Kylin component carries the JAXB runtime that Java 11 removed from the JDK.

## Availability

- The DeepSeek Harness and Pi packages are published on npm: `dsh plugin --profile <profile> add @ottermind/sqlx-dsh` and `pi install npm:@ottermind/sqlx-pi`. Both install the `sqlx` CLI themselves when it is missing.
- The Codex and Claude Code plugins install from the `plugins` branch: `codex plugin marketplace add OtterMind/sqlx@plugins` and `claude plugin marketplace add OtterMind/sqlx@plugins`, then add `sqlx@ottermind`. Their task description now names the new engines.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin, saved datasources and stored results; downloaded components are cached per version and reused, so an upgrade only fetches components that changed. Update managed Skills separately with `sqlx skill update`. The 0.1.17 Skill is published with a `>=0.1.17, <0.2.0` CLI requirement, so update the CLI before the Skill; an older Skill keeps working with the older CLI it was installed with.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.16...v0.1.17
