# CLI updates

SQLX 0.1.2 provides update checks and executable replacement. The CLI remains a process that handles one invocation and exits; updating does not introduce a resident update service or a global SQL task registry.

## Commands

| Command | Behavior |
|---|---|
| `sqlx update check` | Fetch the latest stable release and return current/latest versions, availability and its release URL. No binary is downloaded. |
| `sqlx update install` | Download, verify and install the latest eligible stable CLI. |
| `sqlx update install --version <version>` | Select an exact stable release after the same verification. Downgrades and prereleases are rejected. |
| `sqlx update status` | Read current executable identity and the last check/installation records without networking. |

These commands use the normal `{success, data}` / `{success, error}` JSON envelopes. An available update is a successful check, not a nonzero exit. `up_to_date` means no newer eligible version was installed. Failed checks return `check_failed` in the saved check record and a nonzero exit; they are never reported as no-update.

Installation is synchronous. `installed` is returned only after the new executable is present and its version has been verified at the original path. The old invocation then exits normally. A separate status call is diagnostic, not a required asynchronous installation handoff.

## Background checks

After an ordinary interactive invocation finishes, a short-lived detached process checks for updates if the cache is stale. Both stdin and stdout must be terminals. Help/version parsing, `init`, update commands, local-worker development invocations and piped/CI commands do not trigger this automatically.

Successful observations are cached for 24 hours; failed or interrupted attempts back off for 15 minutes. A later invocation displays a cached notice on stderr at most once per day. The foreground command does not wait for network access and keeps its normal stdout/exit status. Concurrent check processes share a file lock and recheck the cache after obtaining it.

Set `SQLX_NO_UPDATE_CHECK=1` to disable opportunistic checks. Explicit update commands still run when requested. Automatic **installation** is not enabled.

## Distribution and compatibility

Discovery reads `releases/latest/download/release-version.txt` from OtterMind/sqlx. The selected version's manifest, SHA256SUMS and CLI ZIP are then read from its immutable `releases/download/v<version>/` directory. A changed Latest pointer cannot mix files from different versions. No API login, device identifier or SQLX update backend is required.

The CLI package must match the selected stable version, supported platform, expected executable name and versioned GitHub URL. Its SHA-256 must agree between the manifest, SHA256SUMS and actual download. Unsafe archive paths are rejected. The verified candidate must return the exact OtterMind product/version from `--version` within five seconds.

Component `cli_compat` and worker `protocol_version` fields are deliberately not used to reject a replacement CLI: they describe components consumed by a CLI, not whether an older updater can install the newer executable. The updater validates the release/archive format and executable identity instead.

Normal worker/UI dependencies are pinned to the running CLI's versioned release manifest. Checking for a newer CLI does not refresh an active dependency set to Latest. The component `--manifest` override does not change the self-update source. Existing workers, JREs, UI service processes and plugin selections are preserved; new dependencies download only when used. Managed Skills are updated separately with `sqlx skill update`.

This release retains the existing HTTPS/GitHub/checksum trust model. Checksums establish consistency with release metadata, not an independent publisher signature. Signed update metadata is a prerequisite to consider before adding unattended installation.

## Replacement and recovery

SQLX resolves the executable actually running, serializes update writers for that path, stages beside it, and checks that the old file did not change during preparation. Known source-build and package-manager paths and read-only executables are rejected with installation guidance. No administrator escalation is attempted.

The operating-system replacement primitive is [`self-replace` 1.5.0](https://docs.rs/self-replace/1.5.0/self_replace/). Unix uses a sibling file and atomic rename; Windows relocates the running image and places the replacement at the original path. Its temporary helper cleans up the old image after the process exits. Running SQL processes are not killed or restarted.

SQLX preserves an independent backup and its hash before replacement, and records that backup before touching the installed file. It then executes the new binary at the original path to verify the final identity/version. A replacement or verification failure restores the previous executable when possible. If restoration is blocked, the error is `update.recovery_required` and the backup is retained with its path in the record/error. Successful installation history is written before the recovery copy is removed.

Update records live under `~/.sqlx/updates/<executable-path-hash>/`, independently of `--data-dir` database profiles. They contain timestamps, versions, outcomes, paths and errors, not database credentials. `check.json` is the latest observation and `install.json` is the latest installation attempt. An `installing` record without a final timestamp is not proof of success; abnormal termination may require the standalone installer and the retained backup. The updater does not attempt destructive storage migrations or restore old datasource snapshots.

A new version that fails to start at its installed path is an installation failure even if downloading and copying succeeded. If the CLI itself cannot launch, rerun the official installer. Restoring an executable does not undo SQL or a data-format migration.

## Validation

`tests/updates.py` uses disposable copies of the real CLI, native fixture executables and a local release server. It covers fresh checks, offline history, check-cache expiry, noninteractive behavior, POSIX terminal-triggered detached checks, equal/older versions, checksum and archive rejection, candidate timeout, wrong identity, failure after replacement with restoration, actual replacement, preserved datasource files, concurrent updaters and an older CLI invocation completing after replacement. CI runs it on all five supported platform targets.

Test isolation can use `SQLX_UPDATE_DIR` and `SQLX_UPDATE_RELEASE_BASE` (HTTPS or loopback HTTP). These are hidden integration overrides; normal installations use the official release source. Neither override opens or initializes encrypted datasource storage.

For 0.1.0/0.1.1, install an updater-capable version once with the existing installer. Later versions can use the commands above. No new version can retroactively add updater commands to an old executable.
