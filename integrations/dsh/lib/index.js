// OtterMind SQLX tools for DeepSeek Harness.
//
// Every tool shells out to the `sqlx` CLI and returns its JSON result, so the plugin stays a thin
// adapter: credentials, TLS policy, worker downloads and result pages remain CLI responsibilities.
// SQLX_BIN selects a specific executable; otherwise the plugin uses the first `sqlx` on PATH or the
// official user-level installation, and installs the CLI once when neither exists, so adding the
// plugin is the only setup step.
import { execFile } from "node:child_process";
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
async function works(candidate) {
  try {
    await execFileAsync(candidate, ["--version"], { timeout: VERSION_TIMEOUT_MS, env: process.env });
    return true;
  } catch {
    return false;
  }
}
async function resolveExecutable() {
  if (executable) {
    return executable;
  }
  for (const candidate of [process.env.SQLX_BIN, "sqlx", installedPath()].filter(Boolean)) {
    if (await works(candidate)) {
      executable = candidate;
      return executable;
    }
  }
  await execFileAsync("npx", ["-y", "@ottermind/sqlx@latest", "--target", "dsh"], {
    maxBuffer: MAX_BUFFER,
    timeout: INSTALL_TIMEOUT_MS,
    env: process.env,
  });
  const installed = installedPath();
  if (!(await works(installed))) {
    throw new Error(`sqlx is not installed and installing it did not create ${installed}`);
  }
  executable = installed;
  return executable;
}
async function run(args) {
  const { stdout } = await execFileAsync(await resolveExecutable(), args, {
    maxBuffer: MAX_BUFFER,
    env: process.env,
  });
  return JSON.parse(stdout);
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
      return (await run(["datasource", "list"])).data;
    },
  }));
  ctx.tools.register(defineTool({
    name: "sqlx_datasource_show",
    description: "Show one saved SQLX datasource by its UUID or unique name. Never returns usernames or passwords.",
    parameters: { id: { type: "string", required: true, description: "Datasource UUID or unique name" } },
    output: { schema: { type: "object", additionalProperties: true, properties: {} }, render: (_args, value) => text(value) },
    async execute(args) {
      return (await run(["datasource", "show", "--id", args.id])).data;
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
    description: "Download the SQLX components that would otherwise be fetched during a first query or page: mysql, postgres, oracle, sqlserver, ui, skill or all.",
    parameters: {
      components: { type: "array", items: { type: "string" }, required: true, description: "mysql, postgres, oracle, sqlserver, ui, skill or all" },
    },
    output: { schema: { type: "object", additionalProperties: true, properties: {} }, render: (_args, value) => text(value) },
    async execute(args) {
      return await run(["prefetch", ...args.components]);
    },
  }));
}
export { name, inject, apply };
