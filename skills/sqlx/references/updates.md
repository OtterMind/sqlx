# CLI updates

Requires SQLX 0.1.2 or later. For older versions, follow [CLI installation](install-cli.md) once before using these commands.

## Check

```sh
sqlx update check
```

**Purpose:** Discover the latest stable official CLI without installing it.

**Result:** Current/latest version, `update_available` or `up_to_date`, timestamp and release URL. A network or validation failure has a nonzero exit and is not proof that the CLI is current. Normal interactive invocations may show a cached notice on stderr; piped agent calls do not trigger these background checks. `SQLX_NO_UPDATE_CHECK=1` disables opportunistic checks, not explicit update commands.

## Install

```sh
sqlx update install
sqlx update install --version <version>
```

**Purpose:** Install the latest or a selected official stable CLI when the user requests an upgrade. Replace `<version>` with the intended version. Do not run both example commands for one upgrade. Downgrades and prereleases are rejected.

**Result:** `installed` only after download checksums, candidate identity and the executable at the final installed path have been verified. Equal versions are a no-op. Installation targets the current executable and does not stop SQL/UI processes, rewrite connections or update Skills/plugins. A source build or package-manager installation may require its original installation method.

## Inspect a previous attempt

```sh
sqlx update status
```

**Purpose:** Read local check/installation history without making network requests or changing the installation.

**Result:** Current executable path/version and the most recent records, or null records when none exist. If an attempt failed, inspect its error before any retry. `recovery_required` means automatic restoration failed and retained backup information needs attention. An unfinished `installing` record does not establish that the target works. If the executable cannot start, use the official installer instead of attempting to run more commands through it.

Update the managed Skill separately with `sqlx skill update`. Preserve local Skill edits and user-selected UI plugins. Do not retry SQL as part of recovering an update; a binary rollback cannot undo a database write.
