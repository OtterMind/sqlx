import assert from "node:assert/strict";
import { execFile } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

import { platformName } from "../src/release.mjs";
import { makeZip, serve, sha256 } from "./fixtures.mjs";

const PACKAGE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const REPOSITORY = path.resolve(PACKAGE, "..", "..");
const WINDOWS = process.platform === "win32";
const SUFFIX = WINDOWS ? ".exe" : "";
const CLI_NAME = `sqlx${SUFFIX}`;
const CLI = path.join(REPOSITORY, "target", "debug", CLI_NAME);
const SKILL = path.join(REPOSITORY, "skills", "sqlx");
const NPM = WINDOWS ? "npm.cmd" : "npm";
const VERSION = JSON.parse(fs.readFileSync(path.join(PACKAGE, "package.json"), "utf8")).version;
const PLATFORM = platformName();
const EXPECTED_FILES = [
  "package/LICENSE",
  "package/NOTICE",
  "package/README.md",
  "package/bin/sqlx-install.mjs",
  "package/package.json",
  "package/src/args.mjs",
  "package/src/cli.mjs",
  "package/src/errors.mjs",
  "package/src/main.mjs",
  "package/src/release.mjs",
  "package/src/zip.mjs",
];

// Commands stay asynchronous on purpose: the release fixture is served from
// this process, so blocking the event loop would stall the installer's
// downloads.
async function run(command, args, options = {}) {
  try {
    const { stdout, stderr } = await promisify(execFile)(command, args, {
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
      timeout: 600_000,
      ...options,
    });
    return { status: 0, stdout, stderr };
  } catch (error) {
    return {
      status: typeof error.code === "number" ? error.code : 1,
      stdout: error.stdout ?? "",
      stderr: error.stderr ?? error.message,
    };
  }
}

function runNpm(args, options = {}) {
  return run(NPM, args, WINDOWS ? { ...options, shell: true } : options);
}

async function readVersion(binary) {
  const result = await run(binary, ["--version"]);
  return /^sqlx (\S+) \(OtterMind\/sqlx\)$/m.exec(result.stdout)?.[1] ?? null;
}

function tree(root, prefix = "") {
  const entries = [];
  for (const item of fs.readdirSync(root, { withFileTypes: true })) {
    const name = prefix ? `${prefix}/${item.name}` : item.name;
    if (item.isDirectory()) {
      entries.push(...tree(path.join(root, item.name), name));
    } else {
      entries.push({ name, data: fs.readFileSync(path.join(root, item.name)) });
    }
  }
  return entries;
}

function publishRelease(web, base, version, { cliEntries, skillEntries, corruptSums = false }) {
  const release = path.join(web, "download", `v${version}`);
  fs.mkdirSync(release, { recursive: true });
  const cliBytes = makeZip(cliEntries);
  const skillBytes = makeZip(skillEntries);
  const cliFile = `sqlx-${PLATFORM}.zip`;
  const skillFile = `skill-${version}.zip`;
  fs.writeFileSync(path.join(release, cliFile), cliBytes);
  fs.writeFileSync(path.join(release, skillFile), skillBytes);

  const manifest = {
    schema_version: 1,
    components: {
      [`cli:${PLATFORM}`]: {
        version,
        url: `${base}/download/v${version}/${cliFile}`,
        sha256: sha256(cliBytes),
        archive: "zip",
        entrypoint: CLI_NAME,
        cli_compat: ">=0.1.1, <0.2.0",
        protocol_version: 1,
      },
      "skill:any": {
        version,
        url: `${base}/download/v${version}/${skillFile}`,
        sha256: sha256(skillBytes),
        archive: "zip",
        entrypoint: "SKILL.md",
        cli_compat: ">=0.1.1, <0.2.0",
        protocol_version: 1,
      },
    },
  };
  const manifestBytes = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`);
  fs.writeFileSync(path.join(release, "manifest.json"), manifestBytes);
  const sums = [
    `${corruptSums ? "0".repeat(64) : sha256(manifestBytes)}  manifest.json`,
    `${sha256(cliBytes)}  ${cliFile}`,
    `${sha256(skillBytes)}  ${skillFile}`,
  ];
  fs.writeFileSync(path.join(release, "SHA256SUMS"), `${sums.join("\n")}\n`);
  return release;
}

async function pack(root) {
  const destination = path.join(root, "pack");
  fs.mkdirSync(destination, { recursive: true });
  const result = await runNpm(
    ["pack", "--pack-destination", destination, "--cache", path.join(root, "npm-cache"), "--no-audit", "--no-fund"],
    { cwd: PACKAGE },
  );
  assert.equal(result.status, 0, result.stderr);
  const tarball = fs.readdirSync(destination).find((name) => name.endsWith(".tgz"));
  assert.ok(tarball, "npm pack produced no tarball");
  return path.join(destination, tarball);
}

function project(root, name) {
  const directory = path.join(root, name);
  fs.mkdirSync(directory, { recursive: true });
  fs.writeFileSync(path.join(directory, "package.json"), '{\n  "name": "decoy",\n  "version": "1.0.0"\n}\n');
  fs.writeFileSync(path.join(directory, "package-lock.json"), '{\n  "lockfileVersion": 3\n}\n');
  fs.mkdirSync(path.join(directory, "node_modules", "decoy"), { recursive: true });
  fs.writeFileSync(path.join(directory, "node_modules", "decoy", "index.js"), "module.exports = 1;\n");
  return directory;
}

// A stand-in CLI for the version-decision paths, which need executables that
// report versions a real build cannot provide here.
function fakeCli(directory, { version, selfUpdate }) {
  fs.mkdirSync(directory, { recursive: true });
  const binary = path.join(directory, CLI_NAME);
  fs.writeFileSync(path.join(directory, "version.txt"), version);
  if (!selfUpdate) {
    fs.writeFileSync(path.join(directory, "no-self-update"), "");
  }
  fs.writeFileSync(
    binary,
    `#!/bin/sh
directory=$(dirname "$0")
case "$1 $2" in
  "--version ") printf 'sqlx %s (OtterMind/sqlx)\\n' "$(cat "$directory/version.txt")";;
  "update status")
    if [ -f "$directory/no-self-update" ]; then exit 1; fi
    printf '{"current_version":"%s"}\\n' "$(cat "$directory/version.txt")";;
  "update install")
    printf '%s\\n' "$*" >> "$directory/calls.log"
    shift 2
    while [ $# -gt 0 ]; do
      if [ "$1" = "--version" ]; then shift; printf '%s\\n' "$1" > "$directory/version.txt"; fi
      shift
    done
    printf '{"status":"installed"}\\n';;
esac
exit 0
`,
  );
  fs.chmodSync(binary, 0o755);
  return binary;
}

test("packs, installs and refuses what it must refuse", async (t) => {
  assert.ok(fs.existsSync(CLI), `build the CLI first: cargo build --workspace (${CLI})`);
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "sqlx-npm-e2e-"));
  const web = path.join(root, "web");
  fs.mkdirSync(web, { recursive: true });
  const server = await serve(web);
  const base = server.base;
  const realVersion = await readVersion(CLI);
  assert.ok(realVersion, "the debug CLI does not report an OtterMind SQLX version");

  const skillEntries = tree(SKILL);
  publishRelease(web, base, realVersion, {
    cliEntries: [{ name: CLI_NAME, data: fs.readFileSync(CLI), mode: 0o755 }],
    skillEntries,
  });
  publishRelease(web, base, VERSION, {
    cliEntries: [
      {
        name: CLI_NAME,
        data: `#!/bin/sh\nif [ "$1" = "--version" ]; then printf 'sqlx ${VERSION} (OtterMind/sqlx)\\n'; fi\nexit 0\n`,
        mode: 0o755,
      },
    ],
    skillEntries,
  });
  publishRelease(web, base, "0.0.9", {
    cliEntries: [{ name: "unrelated", data: "not the CLI" }],
    skillEntries,
  });
  publishRelease(web, base, "0.0.8", {
    cliEntries: [{ name: CLI_NAME, data: fs.readFileSync(CLI), mode: 0o755 }],
    skillEntries,
    corruptSums: true,
  });

  const home = path.join(root, "home");
  const environment = (version, extra = {}) => ({
    ...process.env,
    HOME: home,
    USERPROFILE: home,
    SQLX_INSTALL_DIR: path.join(root, "bin"),
    SQLX_RELEASE_BASE: base,
    SQLX_DATA_DIR: path.join(root, "data"),
    SQLX_UPDATE_DIR: path.join(root, "updates"),
    SQLX_MANIFEST: `${base}/download/v${version}/manifest.json`,
    SQLX_VERSION: version,
    SQLX_NO_UPDATE_CHECK: "1",
    npm_config_cache: path.join(root, "npm-cache"),
    ...extra,
  });
  const installer = async (args, options) =>
    run(process.execPath, [path.join(PACKAGE, "bin", "sqlx-install.mjs"), ...args], options);
  const downloads = () => server.requests.filter((url) => url.endsWith(".zip")).length;
  const tarball = await pack(root);

  try {
    await t.test("ships only the files the package declares", async () => {
      const result = await run("tar", ["-tzf", tarball]);
      assert.equal(result.status, 0, result.stderr);
      assert.deepEqual(result.stdout.trim().split("\n").sort(), EXPECTED_FILES);
      assert.deepEqual(server.requests, [], "packing must not download anything");
    });

    await t.test("runs the packed tarball through npx", async () => {
      const result = await runNpm(["exec", "--yes", `--package=${tarball}`, "--", "sqlx-install", "--help"], {
        cwd: root,
        env: environment(realVersion),
      });
      assert.equal(result.status, 0, result.stderr);
      assert.match(result.stdout, /Usage:/);
    });

    await t.test("installs the CLI and the Skill from the packed tarball", async () => {
      const directory = project(root, "fresh");
      const result = await runNpm(["exec", "--yes", `--package=${tarball}`, "--", "sqlx-install"], {
        cwd: directory,
        env: environment(realVersion),
      });
      assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
      assert.equal(await readVersion(path.join(root, "bin", CLI_NAME)), realVersion);
      assert.equal(
        sha256(fs.readFileSync(path.join(root, "bin", CLI_NAME))),
        sha256(fs.readFileSync(CLI)),
        "the installed CLI must be the verified release binary",
      );
      assert.ok(fs.existsSync(path.join(directory, "sqlx", "SKILL.md")));
      assert.ok(fs.existsSync(path.join(root, "data")), "the data directory must be created");
      assert.match(result.stdout, /is not in PATH/);
      assert.deepEqual(fs.readdirSync(path.join(root, "bin")).filter((n) => n.startsWith(".sqlx-npm-")), []);
      assert.equal(
        fs.readFileSync(path.join(directory, "package.json"), "utf8"),
        '{\n  "name": "decoy",\n  "version": "1.0.0"\n}\n',
      );
      assert.equal(
        fs.readFileSync(path.join(directory, "package-lock.json"), "utf8"),
        '{\n  "lockfileVersion": 3\n}\n',
      );
      assert.ok(fs.existsSync(path.join(directory, "node_modules", "decoy", "index.js")));
    });

    await t.test("re-runs without replacing the CLI or downloading again", async () => {
      const directory = path.join(root, "fresh");
      const before = sha256(fs.readFileSync(path.join(root, "bin", CLI_NAME)));
      const archives = downloads();
      const result = await installer([], { cwd: directory, env: environment(realVersion) });
      assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
      assert.equal(sha256(fs.readFileSync(path.join(root, "bin", CLI_NAME))), before);
      assert.match(result.stdout, /\(reused\)/);
      assert.equal(downloads(), archives);
    });

    await t.test("installs into every --target shape", async () => {
      const directory = path.join(root, "fresh");
      for (const [target, expected] of [
        ["codex", path.join(home, ".agents", "skills", "sqlx")],
        ["claude", path.join(home, ".claude", "skills", "sqlx")],
        ["relative target/中文 目录", path.join(directory, "relative target", "中文 目录")],
        ["--target=equals form/target", path.join(directory, "equals form", "target")],
      ]) {
        const args = target.startsWith("--target=") ? [target] : ["--target", target];
        const result = await installer(args, { cwd: directory, env: environment(realVersion) });
        assert.equal(result.status, 0, `${target}: ${result.stdout}\n${result.stderr}`);
        assert.ok(fs.existsSync(path.join(expected, "SKILL.md")), `${target} did not install the Skill`);
      }
    });

    await t.test("refuses an unmanaged Skill destination without touching it", async () => {
      const directory = project(root, "conflict");
      fs.mkdirSync(path.join(directory, "sqlx"), { recursive: true });
      fs.writeFileSync(path.join(directory, "sqlx", "SKILL.md"), "user content\n");
      const result = await installer([], { cwd: directory, env: environment(realVersion) });
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /\(skill\)/);
      assert.match(result.stderr, /is installed and usable/);
      assert.equal(fs.readFileSync(path.join(directory, "sqlx", "SKILL.md"), "utf8"), "user content\n");
    });

    await t.test("refuses a release whose SHA256SUMS does not match the manifest", async () => {
      const install = path.join(root, "checksum-bin");
      const result = await installer([], {
        cwd: project(root, "checksum"),
        env: environment("0.0.8", { SQLX_INSTALL_DIR: install }),
      });
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /\(verify\)/);
      assert.equal(fs.existsSync(install), false, "a failed verification must not create the CLI directory");
    });

    await t.test("refuses an archive without the declared entrypoint", async () => {
      const install = path.join(root, "entrypoint-bin");
      const result = await installer([], {
        cwd: project(root, "entrypoint"),
        env: environment("0.0.9", { SQLX_INSTALL_DIR: install }),
      });
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /does not contain/);
      assert.deepEqual(fs.readdirSync(install), [], "a failed install must leave no files behind");
    });

    if (WINDOWS) {
      await t.test("keeps a newer CLI, replaces an older one and self-updates", (context) => {
        context.skip("the version-decision fixtures are POSIX shell executables");
      });
      return;
    }

    await t.test("keeps a newer CLI and does not downgrade", async () => {
      const install = path.join(root, "keep-bin");
      fakeCli(install, { version: "9.9.9", selfUpdate: true });
      const result = await installer([], {
        cwd: project(root, "keep"),
        env: environment(VERSION, { SQLX_INSTALL_DIR: install }),
      });
      assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
      assert.equal(await readVersion(path.join(install, CLI_NAME)), "9.9.9");
      assert.match(result.stdout, /kept newer version/);
      assert.equal(fs.existsSync(path.join(install, "calls.log")), false);
    });

    await t.test("replaces an older CLI that cannot self-update", async () => {
      const install = path.join(root, "replace-bin");
      fakeCli(install, { version: "0.1.0", selfUpdate: false });
      const result = await installer([], {
        cwd: project(root, "replace"),
        env: environment(VERSION, { SQLX_INSTALL_DIR: install }),
      });
      assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
      assert.equal(await readVersion(path.join(install, CLI_NAME)), VERSION);
      assert.match(result.stdout, /\(replaced\)/);
    });

    await t.test("delegates an older CLI that supports self-update", async () => {
      const install = path.join(root, "update-bin");
      fakeCli(install, { version: "0.1.0", selfUpdate: true });
      const result = await installer([], {
        cwd: project(root, "update"),
        env: environment(VERSION, { SQLX_INSTALL_DIR: install }),
      });
      assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
      assert.match(fs.readFileSync(path.join(install, "calls.log"), "utf8"), /update install --version 0\.1\.5/);
      assert.equal(await readVersion(path.join(install, CLI_NAME)), VERSION);
      assert.match(result.stdout, /\(updated\)/);
    });

    await t.test("refuses a foreign executable in the CLI location", async () => {
      const install = path.join(root, "foreign-bin");
      fs.mkdirSync(install, { recursive: true });
      fs.writeFileSync(path.join(install, CLI_NAME), "not a sqlx build\n");
      fs.chmodSync(path.join(install, CLI_NAME), 0o755);
      const result = await installer([], {
        cwd: project(root, "foreign"),
        env: environment(VERSION, { SQLX_INSTALL_DIR: install }),
      });
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /refusing to replace it/);
      assert.equal(fs.readFileSync(path.join(install, CLI_NAME), "utf8"), "not a sqlx build\n");
    });

    await t.test("refuses a symbolic link in the CLI location", async () => {
      const install = path.join(root, "link-bin");
      fs.mkdirSync(install, { recursive: true });
      fs.symlinkSync(CLI, path.join(install, CLI_NAME));
      const result = await installer([], {
        cwd: project(root, "link"),
        env: environment(VERSION, { SQLX_INSTALL_DIR: install }),
      });
      assert.notEqual(result.status, 0);
      assert.match(result.stderr, /symbolic link/);
    });
  } finally {
    await server.close();
    fs.rmSync(root, { recursive: true, force: true });
  }
});
