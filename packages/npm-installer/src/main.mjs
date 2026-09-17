import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { HELP, parseArgs } from "./args.mjs";
import * as cli from "./cli.mjs";
import { InstallError, UsageError } from "./errors.mjs";
import { compareVersions, downloadAsset, loadRelease, platformName, targetVersion } from "./release.mjs";

export function resolveSkillTarget(value, cwd) {
  if (value === undefined) {
    return path.resolve(cwd, "sqlx");
  }
  if (value === "codex") {
    return path.join(os.homedir(), ".agents", "skills", "sqlx");
  }
  if (value === "claude") {
    return path.join(os.homedir(), ".claude", "skills", "sqlx");
  }
  return path.resolve(cwd, value);
}

function pathHint(directory) {
  if (process.platform === "win32") {
    return [
      `${directory} is not in PATH.`,
      `  This session:  $env:Path = "${directory};$env:Path"`,
      "  Permanent:     add that directory to your user PATH",
    ];
  }
  return [
    `${directory} is not in PATH.`,
    `  This shell:  export PATH="${directory}:$PATH"`,
    "  Permanent:   add that line to your shell profile",
  ];
}

function report(progress, log) {
  log.log(`SQLX ${progress.cli.version}`);
  log.log(`  CLI    ${progress.cli.path} (${progress.cli.action})`);
  log.log(`  Skill  ${progress.skill.path}${progress.skill.version ? ` (${progress.skill.version})` : ""}`);
  log.log(`  Data   ${progress.data.path} (${progress.data.created ? "created" : "already initialized"})`);
  const directory = path.dirname(progress.cli.path);
  const entries = (process.env.PATH ?? "").split(path.delimiter).filter(Boolean);
  if (!entries.some((entry) => path.resolve(entry) === directory)) {
    log.log("");
    for (const line of pathHint(directory)) {
      log.log(line);
    }
  }
  log.log("");
  log.log("Next: sqlx --version && sqlx skill status");
}

async function install(options, cwd, log) {
  const version = targetVersion();
  const platform = platformName();
  const binary = path.join(cli.cliDirectory(), cli.binaryName());
  const skillTarget = resolveSkillTarget(options.target, cwd);
  const progress = { cli: null, skill: null, data: null };

  // Refuse an unusable Skill destination before changing anything else.
  let stats = null;
  try {
    stats = fs.lstatSync(skillTarget);
  } catch {
    stats = null;
  }
  if (stats?.isSymbolicLink()) {
    throw new InstallError("skill", `${skillTarget} is a symbolic link; choose another --target`);
  }

  const existing = cli.inspect(binary);
  let action;
  if (!existing.exists) {
    action = "install";
  } else if (existing.version === version) {
    action = "reuse";
  } else if (compareVersions(existing.version, version) > 0) {
    action = "keep";
  } else {
    action = cli.supportsSelfUpdate(binary) ? "update" : "replace";
  }

  if (action === "install" || action === "replace") {
    const release = await loadRelease(version, platform);
    const bytes = await downloadAsset(release.asset);
    cli.installCli({ binary, bytes, entrypoint: release.asset.entrypoint, version });
    progress.cli = { path: binary, version, action: action === "install" ? "installed" : "replaced" };
  } else if (action === "update") {
    cli.selfUpdate(binary, version);
    progress.cli = { path: binary, version, action: "updated" };
  } else if (action === "reuse") {
    progress.cli = { path: binary, version, action: "reused" };
  } else {
    progress.cli = { path: binary, version: existing.version, action: "kept newer version" };
  }

  const dataDirectory = cli.dataDirectory();
  progress.data = { path: dataDirectory, created: cli.initializeData(binary, dataDirectory) };

  try {
    const result = cli.installSkill(binary, skillTarget);
    progress.skill = { path: skillTarget, version: result?.version ?? null };
  } catch (error) {
    throw new InstallError(
      "skill",
      `${error.message}\nThe CLI at ${binary} (${progress.cli.version}) is installed and usable; fix the Skill destination and run the command again to finish the Skill step.`,
      { cause: error },
    );
  }

  report(progress, log);
  return 0;
}

export async function run(argv, log = console) {
  const cwd = process.cwd();
  let options;
  try {
    options = parseArgs(argv);
  } catch (error) {
    if (!(error instanceof UsageError)) {
      throw error;
    }
    log.error(`sqlx-install: ${error.message}`);
    log.error("Run with --help for usage.");
    return 2;
  }
  if (options.help) {
    log.log(HELP);
    return 0;
  }
  try {
    return await install(options, cwd, log);
  } catch (error) {
    const stage = error instanceof InstallError ? error.stage : "install";
    log.error(`SQLX installation failed (${stage}): ${error.message}`);
    return 1;
  }
}
