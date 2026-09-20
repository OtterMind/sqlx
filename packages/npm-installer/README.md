# @ottermind/sqlx

Install or update the [SQLX](https://github.com/OtterMind/sqlx) database CLI and
its agent Skill with one command.

```bash
npx -y @ottermind/sqlx@latest
```

The command installs or updates the native CLI, initializes the local SQLX data
directory when it does not exist yet, and installs the Skill. Node.js is only
needed to run the installer: the CLI itself has no Node.js runtime dependency.

This package contains no binaries. It downloads the CLI and the Skill from the
GitHub Release that matches the version of this package.

## Requirements

- Node.js 22 or newer (`npx` is part of npm).
- Write access to the CLI directory and to the Skill directory.
- Access to the npm Registry and to GitHub Releases.

Windows ARM64 is not a release target.

## Usage

```bash
# Skill in ./sqlx, CLI in the official user-level location
npx -y @ottermind/sqlx@latest

# Skill in the Codex user-level Skill directory
npx -y @ottermind/sqlx@latest --target codex

# Skill in the Claude Code user-level Skill directory
npx -y @ottermind/sqlx@latest --target claude

# Skill in the dsh user-level Skill directory (shared with Codex)
npx -y @ottermind/sqlx@latest --target dsh

# Skill in the Pi user-level Skill directory
npx -y @ottermind/sqlx@latest --target pi

# Skill in an explicit directory
npx -y @ottermind/sqlx@latest --target ./tools/sqlx

npx -y @ottermind/sqlx@latest --help
```

| `--target`          | Skill directory                    |
| ------------------- | ---------------------------------- |
| not given           | `./sqlx` in the current directory  |
| `codex`             | `~/.agents/skills/sqlx`            |
| `dsh`               | `~/.agents/skills/sqlx`            |
| `claude`            | `~/.claude/skills/sqlx`            |
| `pi`                | `~/.pi/agent/skills/sqlx`          |
| any other value     | that path, used as the directory   |

Relative paths are resolved against the directory the command runs from, not
against a temporary download directory.

## Where things are installed

| Item       | Location                                                    |
| ---------- | ----------------------------------------------------------- |
| CLI        | `~/.local/bin/sqlx` (macOS, Linux)                          |
| CLI        | `%LOCALAPPDATA%\Programs\SQLX\sqlx.exe` (Windows)           |
| Skill      | the `--target` directory                                    |
| Local data | `~/.sqlx`, created by `sqlx init` when missing              |

The CLI is reused when it is already at the target version, updated when it is
older and supports self-updates, and left untouched when it is newer: this
installer never downgrades a working CLI and never replaces an executable it
does not recognize as `OtterMind/sqlx`.

The Skill is installed by the verified CLI itself, so it shares the same
`skill-installs.json` record, integrity data and local-modification protection
as `sqlx skill install`. The installer never copies Skill files on its own, so
there is only one update rule.

If the CLI installs successfully but the Skill step fails, the command exits
non-zero, keeps the working CLI and reports the Skill failure separately.

## Environment

| Variable            | Meaning                                                    |
| ------------------- | ---------------------------------------------------------- |
| `SQLX_INSTALL_DIR`  | CLI directory instead of the official location             |
| `SQLX_RELEASE_BASE` | Release download base instead of `github.com`              |
| `SQLX_DATA_DIR`     | SQLX data directory instead of `~/.sqlx`                   |
| `SQLX_VERSION`      | Release to install instead of this package version         |

## Verification

Before anything is written, the installer checks that the release metadata, the
`SHA256SUMS` file and the downloaded archive agree:

1. `SHA256SUMS` is downloaded and `manifest.json` is verified against it.
2. The CLI archive is verified against the SHA-256 in the manifest.
3. The archive is extracted with a ZIP reader that rejects absolute paths, `..`
   traversal, backslashes and symbolic links.
4. The extracted binary is executed once as `--version` and must report
   `OtterMind/sqlx` at the expected version.
5. The binary is staged inside the CLI directory and renamed into place, so a
   failure cannot leave a half-written executable.

## This package does not

- run `preinstall`, `install` or `postinstall` scripts;
- write to the `package.json`, lockfile or `node_modules` of the calling
  project;
- download database workers, drivers or a JRE — the CLI fetches those when a
  connection actually needs them;
- modify `PATH` or shell profiles; it reports the directory to add when the CLI
  is not reachable.

## Troubleshooting

- **`SQLX installation failed (cli)`** — the CLI directory is not writable, or
  it already contains a `sqlx` that is not an OtterMind build. Set
  `SQLX_INSTALL_DIR` to a directory you own.
- **`SQLX installation failed (skill)`** — the Skill destination already exists
  and is not managed by SQLX, or it has local changes. The CLI stays installed;
  fix the destination and run the command again.
- **`SQLX installation failed (download)`** — the GitHub Release for this
  package version is not reachable. Check the version exists before retrying.

No Node.js? Use the Shell or PowerShell installer described in the
[SQLX README](https://github.com/OtterMind/sqlx#readme).

## License

See `LICENSE` and `NOTICE`. For package metadata these terms are identified as
`LicenseRef-Chat2DB`.
