# Install, update and manage the CLI

The repository is https://github.com/OtterMind/sqlx. The supported CLI range for this Skill is `>=0.1.12, <0.2.0`; check `sqlx --version` before relying on a command from this file.

`sqlx setting list` shows the settings in effect, and `sqlx setting set <key> <value>` changes the preview size, the result directory or the result retention; see [results](results.md).

## Install

```sh
npx -y @ottermind/sqlx@latest
export PATH="$HOME/.local/bin:$PATH"
```

The installer downloads the archive for the running platform, verifies it, puts the executable in `~/.local/bin` on macOS and Linux or `%LOCALAPPDATA%\Programs\SQLX` on Windows, and prints the exact PATH line to add when that directory is missing. A shell whose PATH was built earlier still cannot resolve `sqlx`; export the directory or call the absolute path instead of installing again.

The README also publishes release installers, [install.sh](https://github.com/OtterMind/sqlx/blob/main/scripts/install.sh) for macOS/Linux and [install.ps1](https://github.com/OtterMind/sqlx/blob/main/scripts/install.ps1) for Windows x64, which select a fixed release version, verify SHA-256 and preserve unrelated same-name executables.

Prefer one of those two paths. Do the manual steps below only when neither is available, and never download the CLI of another project or pretend a release exists:

1. Check `sqlx --version` and `sqlx --help`. An existing Rust SQLx migration tool is a different executable; preserve it and install this client into a separate user directory, using the explicit path when needed.
2. Select a release from the repository's GitHub Releases page compatible with this Skill. If no binary release is published, follow the [README source installation](https://github.com/OtterMind/sqlx#build-from-source) when the required build tools are available.
3. Select the main CLI archive for the running environment: `macos-arm64`, `macos-x64`, `windows-x64`, `linux-arm64`, or `linux-x64`. Under translation or emulation, check which target can actually execute. Windows ARM64 is not a v1 native target.
4. Download the fixed release's `sqlx-<platform>.zip` and `SHA256SUMS`. Verify the archive before extraction. Unix can use `curl`, `shasum -a 256`/`sha256sum`, and `unzip`; Windows can use `Invoke-WebRequest`, `Get-FileHash -Algorithm SHA256`, and `Expand-Archive`. Do not use credentials or Rust/Node/Java to bootstrap the main binary.
5. Install to a user executable directory, for example `~/.local/bin` on Unix or `%LOCALAPPDATA%\Programs\SQLX` on Windows. On Unix set the executable bit. Add that directory to the user PATH when needed, and check the actual resolved path to avoid a same-name collision.
6. Run the installed executable's `--version`, `--help`, and `init`, then continue the datasource task. Native and JDBC workers download only when required; each download reports progress, speed and elapsed time, retries an interrupted transfer, and can be fetched ahead of time with `sqlx prefetch <component>` when the network is slow. Read `downloads.md` for the component list.

For local development only, build with `cargo build --workspace` and use `SQLX_WORKER_DIR=<checkout>/target/debug`. See the repository README for the JDBC developer setup. Source builds are not required for normal users.

## Manage this Skill

```sh
sqlx skill install --target codex      # also claude, dsh, pi
sqlx skill install --path <skill-directory>
sqlx skill status
sqlx skill update
sqlx skill remove --path <skill-directory>
```

After installing or updating, follow the target agent's discovery or reload mechanism. Codex and dsh share `~/.agents/skills`, Claude Code uses `~/.claude/skills`, and Pi uses `~/.pi/agent/skills`. `status` and `update` operate on SQLX-managed installations and preserve locally modified Skill files; a record whose directory no longer exists is marked `missing: true`. `remove` stops managing that installation without deleting any files.

The main executable can install this Skill with `sqlx skill install`; when the Skill was installed first, it is already usable and that command is not a bootstrap prerequisite. Explicit `--path` names the complete skill folder, such as `<agent-skills>/sqlx`.

## Check for an update

```sh
sqlx update check
```

**Purpose:** Discover the latest stable official CLI without installing it. An update check is not authorization to install.

**Result:** Current/latest version, `update_available` or `up_to_date`, timestamp and release URL. A network or validation failure has a nonzero exit and is not proof that the CLI is current. Normal interactive invocations may show a cached notice on stderr; piped agent calls do not trigger these background checks. `SQLX_NO_UPDATE_CHECK=1` disables opportunistic checks, not explicit update commands.

## Install an update

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

Update the managed Skill separately with `sqlx skill update`, and preserve local Skill edits and user-selected UI plugins. Do not retry SQL as part of recovering an update; a binary rollback cannot undo a database write. If the installed CLI is 0.1.0/0.1.1, rerun the installer to get an updater-capable version; from 0.1.2 the commands above are available.
