// OtterMind SQLX tools for DeepSeek Harness.
//
// Every tool shells out to the `sqlx` CLI and returns its JSON result, so the plugin stays a thin
// adapter: credentials, TLS policy, worker downloads and result pages remain CLI responsibilities.
// SQLX_BIN selects a specific executable; otherwise the first `sqlx` on PATH is used.
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { defineTool } from "@deepseek-ai/dsh-tools";

const execFileAsync = promisify(execFile);
const name = "dsh-sqlx";
const inject = ["tools"];

async function run(args) {
  const { stdout } = await execFileAsync(process.env.SQLX_BIN || "sqlx", args, {
    maxBuffer: 64 * 1024 * 1024,
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
    description: "Execute one or more complete SQL statements through SQLX and return the full structured result. Statements may modify data: call this only after the user authorized that exact operation and scope. Results are never replayed automatically.",
    parameters: {
      datasource: { type: "string", required: true, description: "Datasource UUID or unique name" },
      statements: { type: "array", items: { type: "string" }, required: true, description: "Complete SQL statements, executed in order on one connection" },
    },
    output: { schema: { type: "object", additionalProperties: true, properties: {} }, render: (_args, value) => text(value) },
    async execute(args) {
      const command = ["sql", "execute", "--datasource", args.datasource];
      for (const statement of args.statements) command.push("--sql", statement);
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
      for (const statement of args.statements) command.push("--sql", statement);
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
