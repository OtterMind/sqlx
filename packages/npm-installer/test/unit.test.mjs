import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import { HELP, parseArgs } from "../src/args.mjs";
import { UsageError } from "../src/errors.mjs";
import { resolveSkillTarget } from "../src/main.mjs";
import { compareVersions, platformName, releaseBase, targetVersion } from "../src/release.mjs";
import { extractZip } from "../src/zip.mjs";
import { makeZip, sha256 } from "./fixtures.mjs";

const PACKAGE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const REPOSITORY = path.resolve(PACKAGE, "..", "..");

function temporary() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "sqlx-npm-unit-"));
}

test("parses the documented arguments", () => {
  assert.deepEqual(parseArgs([]), { target: undefined, help: false });
  assert.deepEqual(parseArgs(["--target", "codex"]), { target: "codex", help: false });
  assert.deepEqual(parseArgs(["--target=claude"]), { target: "claude", help: false });
  assert.deepEqual(parseArgs(["--target", "./tools/sqlx"]), { target: "./tools/sqlx", help: false });
  assert.deepEqual(parseArgs(["--help"]), { target: undefined, help: true });
  assert.deepEqual(parseArgs(["-h"]), { target: undefined, help: true });
  // A path that starts with a dash stays a value instead of becoming an option.
  assert.deepEqual(parseArgs(["--target", "--weird"]), { target: "--weird", help: false });
  assert.match(HELP, /--target <codex\|claude\|dsh\|pi\|path>/);
});

test("rejects unusable arguments", () => {
  for (const argv of [
    ["--target"],
    ["--target="],
    ["--target", "codex", "--target", "claude"],
    ["--unknown"],
    ["extra"],
  ]) {
    assert.throws(() => parseArgs(argv), UsageError, `expected ${JSON.stringify(argv)} to be rejected`);
  }
});

test("resolves every Skill target shape", () => {
  const cwd = path.join(os.tmpdir(), "sqlx target cwd");
  assert.equal(resolveSkillTarget(undefined, cwd), path.join(cwd, "sqlx"));
  assert.equal(resolveSkillTarget("codex", cwd), path.join(os.homedir(), ".agents", "skills", "sqlx"));
  assert.equal(resolveSkillTarget("dsh", cwd), path.join(os.homedir(), ".agents", "skills", "sqlx"));
  assert.equal(resolveSkillTarget("claude", cwd), path.join(os.homedir(), ".claude", "skills", "sqlx"));
  assert.equal(resolveSkillTarget("pi", cwd), path.join(os.homedir(), ".pi", "agent", "skills", "sqlx"));
  assert.equal(resolveSkillTarget("relative/target", cwd), path.join(cwd, "relative", "target"));
  const absolute = resolveSkillTarget("/absolute/target", cwd);
  assert.ok(path.isAbsolute(absolute));
  assert.ok(absolute.endsWith(path.join("absolute", "target")));
  assert.equal(resolveSkillTarget("中文 目录", cwd), path.join(cwd, "中文 目录"));
});

test("compares release versions", () => {
  assert.equal(compareVersions("0.1.4", "0.1.5"), -1);
  assert.equal(compareVersions("0.1.5", "0.1.5"), 0);
  assert.equal(compareVersions("0.2.0", "0.1.9"), 1);
  assert.equal(compareVersions("1.0", "1.0.0"), 0);
  assert.throws(() => compareVersions("0.1.4", "latest"));
});

test("reads the package version and the release base", () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(PACKAGE, "package.json"), "utf8"));
  delete process.env.SQLX_VERSION;
  assert.equal(targetVersion(), manifest.version);
  process.env.SQLX_VERSION = "9.9.9";
  assert.equal(targetVersion(), "9.9.9");
  delete process.env.SQLX_VERSION;

  delete process.env.SQLX_RELEASE_BASE;
  assert.equal(releaseBase(), "https://github.com/OtterMind/sqlx/releases");
  process.env.SQLX_RELEASE_BASE = "http://127.0.0.1:1/";
  assert.equal(releaseBase(), "http://127.0.0.1:1");
  delete process.env.SQLX_RELEASE_BASE;

  assert.match(platformName(), /^(macos|linux|windows)-(arm64|x64)$/);
});

test("extracts a normal archive", () => {
  const root = temporary();
  const archive = makeZip([
    { name: "sqlx", data: "binary" },
    { name: "references/local-ui.md", data: "docs" },
    { name: "stored.txt", data: "plain", store: true },
  ]);
  const names = extractZip(archive, root);
  assert.deepEqual(names, ["sqlx", "references/local-ui.md", "stored.txt"]);
  assert.equal(fs.readFileSync(path.join(root, "sqlx"), "utf8"), "binary");
  assert.equal(fs.readFileSync(path.join(root, "references/local-ui.md"), "utf8"), "docs");
});

test("refuses unsafe archives", () => {
  const cases = {
    traversal: makeZip([{ name: "../outside", data: "x" }]),
    absolute: makeZip([{ name: "/outside", data: "x" }]),
    backslash: makeZip([{ name: "..\\outside", data: "x" }]),
    drive: makeZip([{ name: "C:/outside", data: "x" }]),
    symlink: makeZip([{ name: "link", symlink: true }]),
    notAZip: Buffer.from("this is not an archive at all, but it is long enough"),
  };
  for (const [name, archive] of Object.entries(cases)) {
    const root = temporary();
    assert.throws(
      () => extractZip(archive, root),
      (error) => error?.stage === "archive",
      `${name} must be rejected`,
    );
    assert.deepEqual(fs.readdirSync(root), [], `${name} must not write anything`);
  }
});

test("keeps the packaged license identical to the repository license", () => {
  for (const name of ["LICENSE", "NOTICE"]) {
    const packaged = fs.readFileSync(path.join(PACKAGE, name));
    const repository = fs.readFileSync(path.join(REPOSITORY, name));
    assert.equal(sha256(packaged), sha256(repository), `${name} drifted from the repository copy`);
  }
});
