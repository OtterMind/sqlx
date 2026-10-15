#!/usr/bin/env node
// `npx -y @ottermind/sqlx@latest <command>` runs that SQLX command, installing the CLI first when it
// is missing, so an MCP client can point straight at this package without a separate install step.
//
// With no command, or with the installer's own flags, it installs instead: this package has always
// been invoked that way, and `npx -y @ottermind/sqlx@latest` must keep installing the CLI.
import { spawnSync } from "node:child_process";
import path from "node:path";

import { binaryName, cliDirectory, readVersion } from "../src/cli.mjs";
import { run } from "../src/main.mjs";

/// The installer prints its report on stdout, which carries the MCP protocol once the CLI runs here.
const toStderr = {
  log: (...parts) => process.stderr.write(`${parts.join(" ")}\n`),
  error: (...parts) => process.stderr.write(`${parts.join(" ")}\n`),
};

function resolveCli() {
  for (const candidate of [process.env.SQLX_BIN, path.join(cliDirectory(), binaryName()), "sqlx"]) {
    if (candidate && readVersion(candidate) !== null) {
      return candidate;
    }
  }
  return null;
}

async function launch(args) {
  let binary = resolveCli();
  if (!binary) {
    process.stderr.write("SQLX is not installed yet; installing the CLI first.\n");
    // The CLI only: this caller is a client, not an agent, so it has no Skill directory to fill.
    const code = await run(["--cli-only"], toStderr);
    if (code !== 0) {
      return code;
    }
    binary = resolveCli();
  }
  if (!binary) {
    process.stderr.write("SQLX is not installed; run npx -y @ottermind/sqlx@latest first.\n");
    return 127;
  }
  const result = spawnSync(binary, args, { stdio: "inherit", windowsHide: true });
  if (result.error) {
    process.stderr.write(`${result.error.message}\n`);
    return 127;
  }
  return result.status ?? 1;
}

/// The flags the installer accepts. Naming them rather than testing for a leading dash keeps
/// `sqlx --version` a CLI call instead of an installer one.
const INSTALLER_FLAGS = new Set(["-h", "--help", "--cli-only", "--target"]);

const args = process.argv.slice(2);
const first = args[0] ?? "";
const install =
  args.length === 0 || INSTALLER_FLAGS.has(first) || first.startsWith("--target=");
process.exitCode = install ? await run(args) : await launch(args);
