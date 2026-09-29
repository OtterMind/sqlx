SQLX 0.1.18 checks for updates after every invocation and follows the desktop schedule while doing it.

## Update checks

- The opportunistic check no longer requires an interactive terminal. Piped and agent invocations schedule it too; it runs detached with its streams closed, so a caller sees no extra output and never waits for the network. The cached "update available" notice on standard error still appears only in an interactive session.
- Checks follow the repeating schedule the desktop applications use: 30 minutes after the first check, then 1, 2, 4 and 6 hours, and then from 30 minutes again. The round is stored with the check record, so it survives between invocations; a failed check retries after 15 minutes, and an updated CLI starts the schedule over.
- A `CI` environment variable skips the check, so build machines do not run it. `SQLX_NO_UPDATE_CHECK=1` still disables opportunistic checks everywhere; explicit update commands are unaffected.

## Upgrading

From SQLX 0.1.2 or later, run `sqlx update check`, `sqlx update install`, and `sqlx update status`. Versions 0.1.0 and 0.1.1 need the [README installer](https://github.com/OtterMind/sqlx#install-the-cli) first.

CLI updates preserve running UI services and the selected plugin, saved datasources and stored results; downloaded components are cached per version and reused, so an upgrade only fetches components that changed. Update managed Skills separately with `sqlx skill update`. The 0.1.18 Skill is published with a `>=0.1.18, <0.2.0` CLI requirement, so update the CLI before the Skill; an older Skill keeps working with the older CLI it was installed with.

**Full Changelog**: https://github.com/OtterMind/sqlx/compare/v0.1.17...v0.1.18
