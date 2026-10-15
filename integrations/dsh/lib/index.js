// OtterMind SQLX tools for DeepSeek Harness.
//
// Every tool shells out to the `sqlx` CLI and returns its JSON result, so the plugin stays a thin
// adapter: credentials, TLS policy, worker downloads and result pages remain CLI responsibilities.
// SQLX_BIN selects a specific executable; otherwise the plugin uses the first `sqlx` on PATH or the
// official user-level installation, and installs the CLI once when neither exists, so adding the
// plugin is the only setup step.
import { execFile, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";
import { defineTool } from "@deepseek-ai/dsh-tools";

const execFileAsync = promisify(execFile);
const name = "sqlx";
const inject = ["tools"];
const MAX_BUFFER = 64 * 1024 * 1024;
const VERSION_TIMEOUT_MS = 30_000;
const INSTALL_TIMEOUT_MS = 600_000;
// The CLI release this extension is verified against. tests/integrations.py keeps it in step with
// the package version, and an older CLI is replaced rather than used silently.
const CLI_MIN_VERSION = "0.1.21";
// The connection settings a browser setup accepts; credentials are typed into the page instead.
const CONNECTION_ARGUMENTS = [
  ["--type", "database_type"],
  ["--host", "host"],
  ["--port", "port"],
  ["--database", "database"],
  ["--service", "service"],
  ["--tls", "tls"],
];
let executable;

function installedPath() {
  const override = process.env.SQLX_INSTALL_DIR?.trim();
  const directory = override
    ? resolve(override)
    : process.platform === "win32"
      ? join(process.env.LOCALAPPDATA || join(homedir(), "AppData", "Local"), "Programs", "SQLX")
      : join(homedir(), ".local", "bin");
  return join(directory, process.platform === "win32" ? "sqlx.exe" : "sqlx");
}
function olderThan(version, minimum) {
  const left = version.split(".").map(Number);
  const right = minimum.split(".").map(Number);
  for (let index = 0; index < 3; index += 1) {
    if (left[index] !== right[index]) {
      return left[index] < right[index];
    }
  }
  return false;
}
/// The version an executable reports, or null when it is not a runnable OtterMind SQLX.
async function versionOf(candidate) {
  try {
    const { stdout } = await execFileAsync(candidate, ["--version"], {
      timeout: VERSION_TIMEOUT_MS,
      env: process.env,
    });
    const match = /^sqlx (\d+\.\d+\.\d+) \(OtterMind\/sqlx\)$/m.exec(stdout ?? "");
    return match ? match[1] : null;
  } catch {
    return null;
  }
}
function compatible(version) {
  return version !== null && !olderThan(version, CLI_MIN_VERSION);
}
/// The agent needs the Skill to write engine-correct SQL, and a working CLI is the common case, so
/// the Skill is installed on first use rather than only when the CLI is missing. It runs detached:
/// a tool call must not wait for it, and the next session picks it up.
function ensureSkill(binary) {
  if (existsSync(join(homedir(), ".agents", "skills", "sqlx", "SKILL.md"))) {
    return;
  }
  const child = spawn(binary, ["skill", "install", "--target", "dsh"], {
    detached: true,
    stdio: "ignore",
    env: process.env,
  });
  child.unref();
}
async function resolveExecutable() {
  if (executable) {
    return executable;
  }
  // An explicit override is honoured or refused; it is never passed over for another CLI, because
  // silently running a different binary than the one the user named is worse than failing.
  const override = process.env.SQLX_BIN?.trim();
  if (override) {
    const version = await versionOf(override);
    if (version === null) {
      throw new Error(`SQLX_BIN does not point to a runnable OtterMind SQLX CLI: ${override}`);
    }
    if (olderThan(version, CLI_MIN_VERSION)) {
      throw new Error(`SQLX_BIN has SQLX ${version}; this extension requires SQLX >= ${CLI_MIN_VERSION}`);
    }
    executable = override;
    ensureSkill(executable);
    return executable;
  }
  for (const candidate of ["sqlx", installedPath()]) {
    if (compatible(await versionOf(candidate))) {
      executable = candidate;
      ensureSkill(executable);
      return executable;
    }
  }
  await execFileAsync("npx", ["-y", "@ottermind/sqlx@latest", "--target", "dsh"], {
    maxBuffer: MAX_BUFFER,
    timeout: INSTALL_TIMEOUT_MS,
    env: { ...process.env, SQLX_VERSION: CLI_MIN_VERSION },
  });
  const installed = installedPath();
  const version = await versionOf(installed);
  if (!compatible(version)) {
    throw new Error(
      version
        ? `SQLX ${version} at ${installed} is older than the required ${CLI_MIN_VERSION}`
        : `sqlx is not installed and installing it did not create ${installed}`,
    );
  }
  executable = installed;
  ensureSkill(executable);
  return executable;
}
async function run(args) {
  try {
    const { stdout } = await execFileAsync(await resolveExecutable(), args, {
      maxBuffer: MAX_BUFFER,
      env: process.env,
    });
    return JSON.parse(stdout);
  } catch (error) {
    // SQLX exits non-zero when a statement failed but still prints the complete structured result
    // (`success: false` plus `results[].error` and `skipped`) to stdout. Return that JSON instead
    // of the opaque "Command failed" error; fall back to a structured process error only when
    // stdout is not JSON.
    const stdout = typeof error?.stdout === "string" ? error.stdout : "";
    const stderr = typeof error?.stderr === "string" ? error.stderr : "";
    try {
      return JSON.parse(stdout);
    } catch {
      return {
        success: false,
        error: {
          code: "sqlx.process-error",
          message: error instanceof Error ? error.message : String(error),
          stdout,
          stderr,
        },
      };
    }
  }
}
function text(value) {
  return [{ type: "text", text: JSON.stringify(value, null, 2) }];
}
function apply(ctx) {
  ctx.tools.register(defineTool({
    name: "sqlx_datasource_list",
    description: "List the saved SQLX datasources with their non-secret connection settings.",
    parameters: {},
    output: { schema: { type: "object", additionalProperties: true, properties: {} }, render: (_args, value) => text(value) },
    async execute() {
      const value = await run(["datasource", "list"]);
      return value?.data ?? value;
    },
  }));
  ctx.tools.register(defineTool({
    name: "sqlx_datasource_show",
    description: "Show one saved SQLX datasource by its UUID or unique name. Never returns usernames or passwords.",
    parameters: { id: { type: "string", required: true, description: "Datasource UUID or unique name" } },
    output: { schema: { type: "object", additionalProperties: true, properties: {} }, render: (_args, value) => text(value) },
    async execute(args) {
      const value = await run(["datasource", "show", "--id", args.id]);
      return value?.data ?? value;
    },
  }));
  ctx.tools.register(defineTool({
    name: "sqlx_datasource_test",
    description: "Open one connection through SQLX and report whether the datasource is reachable. Executes no SQL.",
    parameters: { id: { type: "string", required: true, description: "Datasource UUID or unique name" } },
    output: { schema: { type: "object", additionalProperties: true, properties: {} }, render: (_args, value) => text(value) },
    async execute(args) {
      return await run(["datasource", "test", "--id", args.id]);
    },
  }));
  ctx.tools.register(defineTool({
    name: "sqlx_datasource_setup",
    description: "Start adding a SQLX datasource that the user finishes in a local browser page, and return that page's url. The connection carries no credentials: the user types the username and password there, so they never enter this conversation. Give the user the complete URL, then poll sqlx_datasource_setup_status with the returned request_id until it reports the outcome.",
    parameters: {
      name: { type: "string", required: true, description: "Name for the new datasource" },
      connection: { type: "object", required: true, description: "Connection settings without credentials: database_type, host, port, database, service, tls, properties" },
    },
    output: { schema: { type: "object", additionalProperties: true, properties: {} }, render: (_args, value) => text(value) },
    async execute(args) {
      const command = ["datasource", "add", "--ui", "--no-open", "--name", args.name];
      for (const [flag, key] of CONNECTION_ARGUMENTS) {
        const value = args.connection?.[key];
        if (value !== undefined && value !== null && value !== "") {
          command.push(flag, String(value));
        }
      }
      for (const [key, value] of Object.entries(args.connection?.properties ?? {})) {
        command.push("--property", `${key}=${value}`);
      }
      return await run(command);
    },
  }));
  ctx.tools.register(defineTool({
    name: "sqlx_datasource_setup_status",
    description: "Report the outcome of a setup started by sqlx_datasource_setup, without retrieving credentials. Read-only: it never contacts the database again.",
    parameters: { request_id: { type: "string", required: true, description: "Setup request UUID from the sqlx_datasource_setup response" } },
    output: { schema: { type: "object", additionalProperties: true, properties: {} }, render: (_args, value) => text(value) },
    async execute(args) {
      return await run(["datasource", "setup-status", "--request-id", args.request_id]);
    },
  }));
  ctx.tools.register(defineTool({
    name: "sqlx_sql_execute",
    description: "Execute one or more complete SQL statements through SQLX and return the table-shaped result: cols, previewed rows and the row count per statement. A result set larger than the preview is stored and pointed at by file; read it with sqlx_results_rows. Statements may modify data: call this only after the user authorized that exact operation and scope. Results are never replayed automatically.",
    parameters: {
      datasource: { type: "string", required: true, description: "Datasource UUID or unique name" },
      statements: { type: "array", items: { type: "string" }, required: true, description: "Complete SQL statements, executed in order on one connection" },
      full: { type: "boolean", description: "Print every row and store nothing, for a caller that cannot read the stored file" },
    },
    output: { schema: { type: "object", additionalProperties: true, properties: {} }, render: (_args, value) => text(value) },
    async execute(args) {
      const command = ["sql", "execute", "--datasource", args.datasource];
      if (args.full) command.push("--full");
      for (const statement of args.statements) command.push("--command", statement);
      return await run(command);
    },
  }));
  ctx.tools.register(defineTool({
    name: "sqlx_sql_view",
    description: "Execute the statements once in the local SQLX service and return a local result-page URL for the user. The same authorization rule as sqlx_sql_execute applies, and the page's Refresh action reruns the batch.",
    parameters: {
      datasource: { type: "string", required: true, description: "Datasource UUID or unique name" },
      statements: { type: "array", items: { type: "string" }, required: true, description: "Complete SQL statements, executed in order on one connection" },
    },
    output: { schema: { type: "object", additionalProperties: true, properties: {} }, render: (_args, value) => text(value) },
    async execute(args) {
      const command = ["sql", "execute", "--datasource", args.datasource, "--view", "--no-open"];
      for (const statement of args.statements) command.push("--command", statement);
      return await run(command);
    },
  }));
  ctx.tools.register(defineTool({
    name: "sqlx_results_rows",
    description: "Read one page of a result that sqlx_sql_execute stored because it did not fit the printed preview. Read-only: it never contacts the database again.",
    parameters: {
      id: { type: "string", required: true, description: "Result UUID from the execute response" },
      statement: { type: "integer", description: "Statement index inside the result, counted from zero" },
      set: { type: "integer", description: "Result set index of that statement, counted from zero" },
      offset: { type: "integer", description: "First row to return" },
      limit: { type: "integer", description: "Rows to return, 50 by default" },
    },
    output: { schema: { type: "object", additionalProperties: true, properties: {} }, render: (_args, value) => text(value) },
    async execute(args) {
      const command = ["results", "rows", "--id", args.id];
      if (args.statement !== undefined) command.push("--statement", String(args.statement));
      if (args.set !== undefined) command.push("--set", String(args.set));
      if (args.offset !== undefined) command.push("--offset", String(args.offset));
      if (args.limit !== undefined) command.push("--limit", String(args.limit));
      return await run(command);
    },
  }));
  ctx.tools.register(defineTool({
    name: "sqlx_prefetch",
    description: "Download the SQLX components that would otherwise be fetched during a first query or page, for example mysql, postgres, oracle, sqlserver, clickhouse, dameng or kingbase. Pass all to fetch every component; run sqlx prefetch --help for the complete list the CLI accepts.",
    parameters: {
      components: { type: "array", items: { type: "string" }, required: true, description: "Component names to download, for example mysql, postgres, oracle, sqlserver, clickhouse, dameng or kingbase; all fetches every component" },
    },
    output: { schema: { type: "object", additionalProperties: true, properties: {} }, render: (_args, value) => text(value) },
    async execute(args) {
      return await run(["prefetch", ...args.components]);
    },
  }));
}
export { name, inject, apply };
