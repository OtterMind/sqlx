import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { InstallError } from "./errors.mjs";
import { extractZip } from "./zip.mjs";

const COMMAND_TIMEOUT_MS = 60_000;
const INSTALL_TIMEOUT_MS = 600_000;
const MANAGED_PARTS = [
  "/target/debug/",
  "/target/release/",
  "/cellar/",
  "/windowsapps/",
  "/scoop/apps/",
  "/chocolatey/",
];
const MANAGED_PREFIXES = ["/nix/store/", "/snap/", "/usr/bin/", "/bin/"];

export function binaryName() {
  return process.platform === "win32" ? "sqlx.exe" : "sqlx";
}

export function cliDirectory() {
  const override = process.env.SQLX_INSTALL_DIR?.trim();
  if (override) {
    return path.resolve(override);
  }
  if (process.platform === "win32") {
    const local = process.env.LOCALAPPDATA || path.join(os.homedir(), "AppData", "Local");
    return path.join(local, "Programs", "SQLX");
  }
  return path.join(os.homedir(), ".local", "bin");
}

export function dataDirectory() {
  const override = process.env.SQLX_DATA_DIR?.trim();
  return override ? path.resolve(override) : path.join(os.homedir(), ".sqlx");
}

function cliFailure(stdout, stderr, status) {
  for (const stream of [stdout, stderr]) {
    try {
      const message = JSON.parse(stream)?.error?.message;
      if (typeof message === "string" && message !== "") {
        return message;
      }
    } catch {
      // Not a JSON result; fall back to the raw output.
    }
  }
  return `${stdout ?? ""}${stderr ?? ""}`.trim() || `exit code ${status}`;
}

function run(binary, args, timeout, stage) {
  const result = spawnSync(binary, args, {
    encoding: "utf8",
    timeout,
    windowsHide: true,
    env: { ...process.env, SQLX_NO_UPDATE_CHECK: "1" },
  });
  if (result.error) {
    throw new InstallError(stage, `${path.basename(binary)} ${args.join(" ")} failed: ${result.error.message}`);
  }
  if (result.status !== 0) {
    throw new InstallError(
      stage,
      `${path.basename(binary)} ${args.join(" ")} failed: ${cliFailure(result.stdout, result.stderr, result.status)}`,
    );
  }
  return result.stdout ?? "";
}

export function readVersion(binary) {
  const result = spawnSync(binary, ["--version"], {
    encoding: "utf8",
    timeout: COMMAND_TIMEOUT_MS,
    windowsHide: true,
  });
  if (result.error || result.status !== 0) {
    return null;
  }
  const match = /^sqlx (\S+) \(OtterMind\/sqlx\)$/m.exec(result.stdout ?? "");
  return match ? match[1] : null;
}

export function inspect(binary) {
  let stats;
  try {
    stats = fs.lstatSync(binary);
  } catch {
    return { path: binary, exists: false };
  }
  if (stats.isSymbolicLink()) {
    throw new InstallError(
      "cli",
      `${binary} is a symbolic link; install SQLX with the tool that owns it, or set SQLX_INSTALL_DIR`,
    );
  }
  if (!stats.isFile()) {
    throw new InstallError("cli", `${binary} is not a regular file`);
  }
  const version = readVersion(binary);
  if (!version) {
    throw new InstallError(
      "cli",
      `${binary} already exists and is not an OtterMind SQLX executable; refusing to replace it`,
    );
  }
  return { path: binary, exists: true, version };
}

function assertReplaceable(binary) {
  const normalized = binary.replaceAll("\\", "/").toLowerCase();
  if (
    MANAGED_PARTS.some((part) => normalized.includes(part)) ||
    MANAGED_PREFIXES.some((prefix) => normalized.startsWith(prefix))
  ) {
    throw new InstallError(
      "cli",
      `${binary} is owned by a package manager or a source build; update SQLX with that tool`,
    );
  }
  if ((fs.statSync(binary).mode & 0o222) === 0) {
    throw new InstallError("cli", `${binary} is read-only; update SQLX with the tool that installed it`);
  }
}

export function supportsSelfUpdate(binary) {
  const result = spawnSync(binary, ["update", "status"], {
    encoding: "utf8",
    timeout: COMMAND_TIMEOUT_MS,
    windowsHide: true,
  });
  if (result.error || result.status !== 0) {
    return false;
  }
  try {
    return typeof JSON.parse(result.stdout)?.current_version === "string";
  } catch {
    return false;
  }
}

function replace(candidate, target) {
  if (process.platform !== "win32" || !fs.existsSync(target)) {
    fs.renameSync(candidate, target);
    return;
  }
  // Windows cannot rename over a running executable, so keep an independent copy
  // until the replacement is in place.
  const backup = `${target}.sqlx-previous`;
  fs.rmSync(backup, { force: true });
  fs.renameSync(target, backup);
  try {
    fs.renameSync(candidate, target);
  } catch (error) {
    fs.renameSync(backup, target);
    throw error;
  }
  fs.rmSync(backup, { force: true });
}

export function selfUpdate(binary, version) {
  run(binary, ["update", "install", "--version", version], INSTALL_TIMEOUT_MS, "cli");
  const installed = readVersion(binary);
  if (installed !== version) {
    throw new InstallError(
      "cli",
      `sqlx update install finished but ${binary} reports ${installed ?? "no version"}`,
    );
  }
}

export function installCli({ binary, bytes, entrypoint, version }) {
  const directory = path.dirname(binary);
  try {
    fs.mkdirSync(directory, { recursive: true });
  } catch (error) {
    throw new InstallError("cli", `cannot create ${directory}: ${error.message}`);
  }
  if (fs.existsSync(binary)) {
    inspect(binary);
    assertReplaceable(binary);
  }
  let stage;
  try {
    stage = fs.mkdtempSync(path.join(directory, ".sqlx-npm-"));
  } catch (error) {
    throw new InstallError("cli", `cannot write to ${directory}: ${error.message}`);
  }
  try {
    extractZip(bytes, stage);
    const candidate = path.join(stage, entrypoint);
    if (!fs.existsSync(candidate)) {
      throw new InstallError("archive", `archive does not contain ${entrypoint}`);
    }
    fs.chmodSync(candidate, 0o755);
    const candidateVersion = readVersion(candidate);
    if (candidateVersion !== version) {
      throw new InstallError(
        "verify",
        `downloaded CLI reports ${candidateVersion ?? "no version"}, expected ${version}`,
      );
    }
    replace(candidate, binary);
  } finally {
    fs.rmSync(stage, { recursive: true, force: true });
  }
  const installed = readVersion(binary);
  if (installed !== version) {
    throw new InstallError("cli", `installed ${binary} reports ${installed ?? "no version"}`);
  }
}

export function initializeData(binary, directory) {
  if (fs.existsSync(directory)) {
    return false;
  }
  run(binary, ["init"], INSTALL_TIMEOUT_MS, "init");
  return true;
}

export function installSkill(binary, target) {
  const stdout = run(binary, ["skill", "install", "--path", target], INSTALL_TIMEOUT_MS, "skill");
  try {
    return JSON.parse(stdout);
  } catch {
    return null;
  }
}
