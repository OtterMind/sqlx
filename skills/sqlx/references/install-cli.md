# Install OtterMind SQLX

The repository is https://github.com/OtterMind/sqlx. The supported CLI range for this Skill is `>=0.1.11, <0.2.0`. The local-page commands are not available in 0.1.0; check the actual version before using them.

1. Check `sqlx --version` and `sqlx --help`. An existing Rust SQLx migration tool is a different executable; preserve it and install this client into a separate user directory, using the explicit path when needed.
2. Select a release from the repository's GitHub Releases page compatible with this Skill. If no binary release is published, follow the [README source installation](https://github.com/OtterMind/sqlx#build-from-source) when the required build tools are available. Do not download another project's CLI or pretend a release exists.
3. Select the main CLI archive for the running environment: `macos-arm64`, `macos-x64`, `windows-x64`, `linux-arm64`, or `linux-x64`. Under translation or emulation, check which target can actually execute. Windows ARM64 is not a v1 native target.
4. Download the fixed release's `sqlx-<platform>.zip` and `SHA256SUMS`. Verify the archive before extraction. Unix can use `curl`, `shasum -a 256`/`sha256sum`, and `unzip`; Windows can use `Invoke-WebRequest`, `Get-FileHash -Algorithm SHA256`, and `Expand-Archive`. Do not use credentials or Rust/Node/Java to bootstrap the main binary.
5. Install to a user executable directory, for example `~/.local/bin` on Unix or `%LOCALAPPDATA%\Programs\SQLX` on Windows. On Unix set the executable bit. Add that directory to the user PATH when needed, and check the actual resolved path to avoid a same-name collision. A changed persistent PATH may require refreshing the current agent shell. `npx -y @ottermind/sqlx@latest` installs to those directories and prints the exact PATH line for the current shell and profile when the directory is missing; if this shell still cannot resolve `sqlx`, export that directory or call the absolute path before continuing.
6. Run the installed executable's `--version`, `--help`, and `init`. Continue the datasource task. Native and JDBC workers download only when required. Each download reports progress, speed and elapsed time, retries an interrupted transfer, and can be fetched ahead of time with `sqlx prefetch <component>` when the network is slow.

The main executable can subsequently install this Skill with `sqlx skill install --target <agent>` (`codex`, `claude`, `dsh` or `pi`) or `--path <skill-directory>`. When this Skill was installed first, it is already usable; invoking that command is not a bootstrap prerequisite. Explicit `--path` should name the complete skill folder, such as `<agent-skills>/sqlx`.

For local development only, build with `cargo build --workspace` and use `SQLX_WORKER_DIR=<checkout>/target/debug`. See the repository README for the JDBC developer setup. Source builds are not required for normal users.

The README provides copyable installation commands for both operating-system families. Release installers are [install.sh](https://github.com/OtterMind/sqlx/blob/main/scripts/install.sh) for macOS/Linux and [install.ps1](https://github.com/OtterMind/sqlx/blob/main/scripts/install.ps1) for Windows x64. They select a fixed release version, verify SHA-256, install to a user directory, and preserve unrelated same-name executables. Add the printed directory to the current shell's PATH and verify the actual executable before initialization.

If the installed CLI is 0.1.0/0.1.1, rerun the installer to get an updater-capable version. From 0.1.2, use `sqlx update check` and an explicitly requested `sqlx update install`; see [CLI updates](updates.md).

## Manage this Skill

```sh
sqlx skill install --target codex      # also claude, dsh, pi
sqlx skill install --path <skill-directory>
sqlx skill status
sqlx skill update
sqlx skill remove --path <skill-directory>
```

After installing or updating, follow the target agent's discovery or reload mechanism. Codex and dsh share `~/.agents/skills`, Claude Code uses `~/.claude/skills`, and Pi uses `~/.pi/agent/skills`. `status` and `update` operate on SQLX-managed installations and preserve locally modified Skill files; a record whose directory no longer exists is marked `missing: true`. `remove` stops managing that installation without deleting any files.
