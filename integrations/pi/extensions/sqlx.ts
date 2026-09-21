/**
 * OtterMind SQLX tools for Pi.
 *
 * Each tool shells out to the `sqlx` CLI and returns its JSON result, so this extension stays a
 * thin adapter: credentials, TLS policy, worker downloads and result pages remain CLI concerns.
 * SQLX_BIN selects a specific executable; otherwise the extension uses the first `sqlx` on PATH or
 * the official user-level installation, and installs the CLI once when neither exists, so
 * installing the package is the only setup step.
 */
import { execFile } from "node:child_process";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

const execFileAsync = promisify(execFile);
const MAX_BUFFER = 64 * 1024 * 1024;
const VERSION_TIMEOUT_MS = 30_000;
const INSTALL_TIMEOUT_MS = 600_000;
let executable: string | undefined;

function installedPath(): string {
	const override = process.env.SQLX_INSTALL_DIR?.trim();
	const directory = override
		? resolve(override)
		: process.platform === "win32"
			? join(process.env.LOCALAPPDATA || join(homedir(), "AppData", "Local"), "Programs", "SQLX")
			: join(homedir(), ".local", "bin");
	return join(directory, process.platform === "win32" ? "sqlx.exe" : "sqlx");
}
async function works(candidate: string): Promise<boolean> {
	try {
		await execFileAsync(candidate, ["--version"], { timeout: VERSION_TIMEOUT_MS, env: process.env });
		return true;
	} catch {
		return false;
	}
}
async function resolveExecutable(): Promise<string> {
	if (executable) {
		return executable;
	}
	for (const candidate of [process.env.SQLX_BIN, "sqlx", installedPath()].filter(
		(value): value is string => Boolean(value),
	)) {
		if (await works(candidate)) {
			executable = candidate;
			return executable;
		}
	}
	await execFileAsync("npx", ["-y", "@ottermind/sqlx@latest", "--target", "pi"], {
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

async function sqlx(args: string[]): Promise<any> {
	const { stdout } = await execFileAsync(await resolveExecutable(), args, {
		maxBuffer: MAX_BUFFER,
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
		name: "sqlx_datasource_show",
		label: "SQLX datasource",
		description:
			"Show one saved OtterMind SQLX datasource by its UUID or unique name. Never returns usernames or passwords.",
		parameters: Type.Object({
			id: Type.String({ description: "Datasource UUID or unique name" }),
		}),
		async execute(_toolCallId, params) {
			return result((await sqlx(["datasource", "show", "--id", params.id])).data);
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
