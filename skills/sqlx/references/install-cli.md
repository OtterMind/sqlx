# Install OtterMind SQLX

The repository is https://github.com/OtterMind/sqlx. The supported CLI range for this Skill is `>=0.1.0, <0.2.0`.

1. Check `sqlx --version` and `sqlx --help`. An existing Rust SQLx migration tool is a different executable; preserve it and install this client into a separate user directory, using the explicit path when needed.
2. Select a release from the repository's GitHub Releases page compatible with this Skill. If no release is published, say the distribution is not yet available; do not download another project's CLI.
3. Select the main CLI archive for the running environment: `macos-arm64`, `macos-x64`, `windows-x64`, `linux-arm64`, or `linux-x64`. Under translation or emulation, check which target can actually execute. Windows ARM64 is not a v1 native target.
4. Download the fixed release's `sqlx-<platform>.zip` and `SHA256SUMS`. Verify the archive before extraction. Unix can use `curl`, `shasum -a 256`/`sha256sum`, and `unzip`; Windows can use `Invoke-WebRequest`, `Get-FileHash -Algorithm SHA256`, and `Expand-Archive`. Do not use credentials or Rust/Node/Java to bootstrap the main binary.
5. Install to a user executable directory, for example `~/.local/bin` on Unix or `%LOCALAPPDATA%\Programs\SQLX` on Windows. On Unix set the executable bit. Add that directory to the user PATH when needed, and check the actual resolved path to avoid a same-name collision. A changed persistent PATH may require refreshing the current agent shell.
6. Run the installed executable's `--version`, `--help`, and `init`. Continue the datasource task. Native and JDBC workers download only when required.

The main executable can subsequently install this Skill with `sqlx skill install --target <agent>` or `--path <skill-directory>`. When this Skill was installed first, it is already usable; invoking that command is not a bootstrap prerequisite. Explicit `--path` should name the complete skill folder, such as `<agent-skills>/sqlx`.

For local development only, build with `cargo build --workspace` and use `SQLX_WORKER_DIR=<checkout>/target/debug`. See the repository README for the JDBC developer setup. Source builds are not required for normal users.
