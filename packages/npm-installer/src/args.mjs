import { UsageError } from "./errors.mjs";

export const HELP = `Install or update the SQLX CLI and its agent Skill.

Usage:
  npx -y @ottermind/sqlx@latest [--target <codex|claude|path>]

Options:
  --target <codex|claude|path>  Skill destination. Without it the Skill is
                                installed to ./sqlx in the directory the command
                                runs from. "codex" and "claude" select the
                                user-level Skill directory of that agent; any
                                other value is used as the Skill directory.
  -h, --help                    Show this help and exit.

The CLI is installed to its official user-level location:
  macOS, Linux  ~/.local/bin/sqlx
  Windows       %LOCALAPPDATA%\\Programs\\SQLX\\sqlx.exe

This package never runs an install script, never downloads inside a project and
never edits the package.json, lockfile or node_modules of the calling project.

Environment:
  SQLX_INSTALL_DIR   CLI directory instead of the official location.
  SQLX_RELEASE_BASE  Release download base instead of github.com.
  SQLX_DATA_DIR      SQLX data directory instead of ~/.sqlx.
  SQLX_VERSION       Release to install instead of this package version.

Examples:
  npx -y @ottermind/sqlx@latest
  npx -y @ottermind/sqlx@latest --target codex
  npx -y @ottermind/sqlx@latest --target ./tools/sqlx
`;

export function parseArgs(argv) {
  const options = { target: undefined, help: false };
  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    if (argument === "--help" || argument === "-h") {
      options.help = true;
      continue;
    }
    if (argument === "--target" || argument.startsWith("--target=")) {
      if (options.target !== undefined) {
        throw new UsageError("--target was given more than once");
      }
      if (argument === "--target") {
        index += 1;
        if (index >= argv.length) {
          throw new UsageError("--target needs codex, claude or a directory");
        }
        options.target = argv[index];
      } else {
        options.target = argument.slice("--target=".length);
      }
      if (options.target === "") {
        throw new UsageError("--target needs codex, claude or a directory");
      }
      continue;
    }
    throw new UsageError(
      argument.startsWith("-") ? `unknown option: ${argument}` : `unexpected argument: ${argument}`,
    );
  }
  return options;
}
