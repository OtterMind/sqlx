/**
 * OtterMind SQLX tools for Pi.
 *
 * Each tool shells out to the `sqlx` CLI and returns its JSON result, so this extension stays a
 * thin adapter: credentials, TLS policy, worker downloads and result pages remain CLI concerns.
 * SQLX_BIN selects a specific executable; otherwise the first `sqlx` on PATH is used.
 */
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

const execFileAsync = promisify(execFile);

async function sqlx(args: string[]): Promise<any> {
	const { stdout } = await execFileAsync(process.env.SQLX_BIN || "sqlx", args, {
		maxBuffer: 64 * 1024 * 1024,
		env: process.env,
	});
	return JSON.parse(stdout);
}
function result(value: unknown) {
	return { content: [{ type: "text" as const, text: JSON.stringify(value, null, 2) }] };
}

export default function (pi: ExtensionAPI) {
	pi.registerTool({
		name: "sqlx_datasource_list",
		label: "SQLX datasources",
		description: "List the saved OtterMind SQLX datasources with their non-secret connection settings.",
		parameters: Type.Object({}),
		async execute() {
			return result((await sqlx(["datasource", "list"])).data);
		},
	});

	pi.registerTool({
		name: "sqlx_datasource_test",
		label: "SQLX connection test",
		description: "Open one connection through SQLX and report whether the datasource is reachable. Executes no SQL.",
		parameters: Type.Object({
			id: Type.String({ description: "Datasource UUID or unique name" }),
		}),
		async execute(_toolCallId, params) {
			return result(await sqlx(["datasource", "test", "--id", params.id]));
		},
	});

	pi.registerTool({
		name: "sqlx_sql_execute",
		label: "SQLX SQL execution",
		description:
			"Execute one or more complete SQL statements through SQLX and return the full structured result. Statements may modify data: call this only after the user authorized that exact operation and scope. Results are never replayed automatically.",
		parameters: Type.Object({
			datasource: Type.String({ description: "Datasource UUID or unique name" }),
			statements: Type.Array(Type.String(), {
				description: "Complete SQL statements, executed in order on one connection",
			}),
		}),
		async execute(_toolCallId, params) {
			const args = ["sql", "execute", "--datasource", params.datasource];
			for (const statement of params.statements) args.push("--sql", statement);
			return result(await sqlx(args));
		},
	});

	pi.registerTool({
		name: "sqlx_sql_view",
		label: "SQLX result page",
		description:
			"Execute the statements once in the local SQLX service and return a local result-page URL for the user. The same authorization rule as sqlx_sql_execute applies, and the page's Refresh action reruns the batch.",
		parameters: Type.Object({
			datasource: Type.String({ description: "Datasource UUID or unique name" }),
			statements: Type.Array(Type.String(), {
				description: "Complete SQL statements, executed in order on one connection",
			}),
		}),
		async execute(_toolCallId, params) {
			const args = ["sql", "execute", "--datasource", params.datasource, "--view", "--no-open"];
			for (const statement of params.statements) args.push("--sql", statement);
			return result(await sqlx(args));
		},
	});

	pi.registerTool({
		name: "sqlx_prefetch",
		label: "SQLX prefetch",
		description:
			"Download the SQLX components that would otherwise be fetched during a first query or page: mysql, postgres, oracle, sqlserver, ui, skill or all.",
		parameters: Type.Object({
			components: Type.Array(Type.String(), {
				description: "mysql, postgres, oracle, sqlserver, ui, skill or all",
			}),
		}),
		async execute(_toolCallId, params) {
			return result(await sqlx(["prefetch", ...params.components]));
		},
	});
}
