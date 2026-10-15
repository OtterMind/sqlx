/**
 * OtterMind SQLX tools for Pi.
 *
 * Each tool shells out to the `sqlx` CLI and returns its JSON result, so this extension stays a
 * thin adapter: credentials, TLS policy, worker downloads and result pages remain CLI concerns.
 * SQLX_BIN selects a specific executable; otherwise the extension uses the first `sqlx` on PATH or
 * the official user-level installation, and installs the CLI once when neither exists, so
 * installing the package is the only setup step.
 */
import { execFile, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";
import type { ExtensionAPI } from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";

const execFileAsync = promisify(execFile);
const MAX_BUFFER = 64 * 1024 * 1024;
const VERSION_TIMEOUT_MS = 30_000;
const INSTALL_TIMEOUT_MS = 600_000;
// The CLI release this extension is verified against. tests/integrations.py keeps it in step with
// the package version, and an older CLI is replaced rather than used silently.
const CLI_MIN_VERSION = "0.1.20";
// The connection settings a browser setup accepts; credentials are typed into the page instead.
const CONNECTION_ARGUMENTS: [string, string][] = [
	["--type", "database_type"],
	["--host", "host"],
	["--port", "port"],
	["--database", "database"],
	["--service", "service"],
	["--tls", "tls"],
];
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
function olderThan(version: string, minimum: string): boolean {
	const left = version.split(".").map(Number);
	const right = minimum.split(".").map(Number);
	for (let index = 0; index < 3; index += 1) {
		if (left[index] !== right[index]) {
			return left[index] < right[index];
		}
	}
	return false;
}
// The version an executable reports, or null when it is not a runnable OtterMind SQLX.
async function versionOf(candidate: string): Promise<string | null> {
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
function compatible(version: string | null): boolean {
	return version !== null && !olderThan(version, CLI_MIN_VERSION);
}
// The agent needs the Skill to write engine-correct SQL, and a working CLI is the common case, so
// the Skill is installed on first use rather than only when the CLI is missing. It runs detached:
// a tool call must not wait for it, and the next session picks it up.
function ensureSkill(binary: string): void {
	if (existsSync(join(homedir(), ".pi", "agent", "skills", "sqlx", "SKILL.md"))) {
		return;
	}
	const child = spawn(binary, ["skill", "install", "--target", "pi"], {
		detached: true,
		stdio: "ignore",
		env: process.env,
	});
	child.unref();
}
async function resolveExecutable(): Promise<string> {
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
	await execFileAsync("npx", ["-y", "@ottermind/sqlx@latest", "--target", "pi"], {
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

async function sqlx(args: string[]): Promise<any> {
	try {
		const { stdout } = await execFileAsync(await resolveExecutable(), args, {
			maxBuffer: MAX_BUFFER,
			env: process.env,
		});
		return JSON.parse(stdout);
	} catch (error) {
		// SQLX exits non-zero when a statement failed but still prints the complete structured
		// result (`success: false` plus `results[].error` and `skipped`) to stdout. Return that
		// JSON instead of the opaque "Command failed" error; fall back to a structured process
		// error only when stdout is not JSON.
		const failure = (error ?? {}) as { stdout?: unknown; stderr?: unknown; message?: unknown };
		const stdout = typeof failure.stdout === "string" ? failure.stdout : "";
		const stderr = typeof failure.stderr === "string" ? failure.stderr : "";
		try {
			return JSON.parse(stdout);
		} catch {
			return {
				success: false,
				error: {
					code: "sqlx.process-error",
					message: typeof failure.message === "string" ? failure.message : String(error),
					stdout,
					stderr,
				},
			};
		}
	}
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
		annotations: { readOnlyHint: true, openWorldHint: false },
		async execute() {
			const value = await sqlx(["datasource", "list"]);
			return result(value?.data ?? value);
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
		annotations: { readOnlyHint: true, openWorldHint: false },
		async execute(_toolCallId, params) {
			const value = await sqlx(["datasource", "show", "--id", params.id]);
			return result(value?.data ?? value);
		},
	});

	pi.registerTool({
		name: "sqlx_datasource_test",
		label: "SQLX connection test",
		description: "Open one connection through SQLX and report whether the datasource is reachable. Executes no SQL.",
		parameters: Type.Object({
			id: Type.String({ description: "Datasource UUID or unique name" }),
		}),
		annotations: { readOnlyHint: true, openWorldHint: true },
		async execute(_toolCallId, params) {
			return result(await sqlx(["datasource", "test", "--id", params.id]));
		},
	});

	pi.registerTool({
		name: "sqlx_datasource_setup",
		label: "SQLX datasource setup",
		description:
			"Start adding a SQLX datasource that the user finishes in a local browser page, and return that page's url. The connection carries no credentials: the user types the username and password there, so they never enter this conversation. Give the user the complete URL, then poll sqlx_datasource_setup_status with the returned request_id until it reports the outcome.",
		parameters: Type.Object({
			name: Type.String({ description: "Name for the new datasource" }),
			connection: Type.Object(
				{
					database_type: Type.String({ description: "Engine, such as postgresql or mysql" }),
					host: Type.Optional(Type.String()),
					port: Type.Optional(Type.Number()),
					database: Type.Optional(Type.String()),
					service: Type.Optional(Type.String()),
					tls: Type.Optional(Type.String()),
					properties: Type.Optional(Type.Record(Type.String(), Type.String())),
				},
				{ description: "Connection settings without credentials" },
			),
		}),
		annotations: { readOnlyHint: false, destructiveHint: true, openWorldHint: true },
		async execute(_toolCallId, params) {
			const connection = params.connection as Record<string, unknown>;
			const command = ["datasource", "add", "--ui", "--no-open", "--name", params.name];
			for (const [flag, key] of CONNECTION_ARGUMENTS) {
				const value = connection?.[key];
				if (value !== undefined && value !== null && value !== "") {
					command.push(flag, String(value));
				}
			}
			for (const [key, value] of Object.entries(
				(connection?.properties ?? {}) as Record<string, unknown>,
			)) {
				command.push("--property", `${key}=${value}`);
			}
			return result(await sqlx(command));
		},
	});

	pi.registerTool({
		name: "sqlx_datasource_setup_status",
		label: "SQLX setup status",
		description:
			"Report the outcome of a setup started by sqlx_datasource_setup, without retrieving credentials. Read-only: it never contacts the database again.",
		parameters: Type.Object({
			request_id: Type.String({ description: "Setup request UUID from the sqlx_datasource_setup response" }),
		}),
		annotations: { readOnlyHint: true, openWorldHint: false },
		async execute(_toolCallId, params) {
			return result(await sqlx(["datasource", "setup-status", "--request-id", params.request_id]));
		},
	});

	pi.registerTool({
		name: "sqlx_sql_execute",
		label: "SQLX SQL execution",
		description:
			"Execute one or more complete SQL statements through SQLX and return the table-shaped result: cols, previewed rows and the row count per statement. A result set larger than the preview is stored and pointed at by file; read it with sqlx_results_rows. Statements may modify data: call this only after the user authorized that exact operation and scope. Results are never replayed automatically.",
		parameters: Type.Object({
			datasource: Type.String({ description: "Datasource UUID or unique name" }),
			statements: Type.Array(Type.String(), {
				description: "Complete SQL statements, executed in order on one connection",
			}),
			full: Type.Optional(
				Type.Boolean({
					description: "Print every row and store nothing, for a caller that cannot read the stored file",
				}),
			),
		}),
		annotations: { readOnlyHint: false, destructiveHint: true, openWorldHint: true },
		async execute(_toolCallId, params) {
			const args = ["sql", "execute", "--datasource", params.datasource];
			if (params.full) args.push("--full");
			for (const statement of params.statements) args.push("--command", statement);
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
		annotations: { readOnlyHint: false, destructiveHint: true, openWorldHint: true },
		async execute(_toolCallId, params) {
			const args = ["sql", "execute", "--datasource", params.datasource, "--view", "--no-open"];
			for (const statement of params.statements) args.push("--command", statement);
			return result(await sqlx(args));
		},
	});

	pi.registerTool({
		name: "sqlx_results_rows",
		label: "SQLX stored result rows",
		description:
			"Read one page of a result that sqlx_sql_execute stored because it did not fit the printed preview. Read-only: it never contacts the database again.",
		parameters: Type.Object({
			id: Type.String({ description: "Result UUID from the execute response" }),
			statement: Type.Optional(Type.Number({ description: "Statement index inside the result, counted from zero" })),
			set: Type.Optional(Type.Number({ description: "Result set index of that statement, counted from zero" })),
			offset: Type.Optional(Type.Number({ description: "First row to return" })),
			limit: Type.Optional(Type.Number({ description: "Rows to return, 50 by default" })),
		}),
		annotations: { readOnlyHint: true, destructiveHint: false, openWorldHint: false },
		async execute(_toolCallId, params) {
			const args = ["results", "rows", "--id", params.id];
			if (params.statement !== undefined) args.push("--statement", String(params.statement));
			if (params.set !== undefined) args.push("--set", String(params.set));
			if (params.offset !== undefined) args.push("--offset", String(params.offset));
			if (params.limit !== undefined) args.push("--limit", String(params.limit));
			return result(await sqlx(args));
		},
	});

	pi.registerTool({
		name: "sqlx_prefetch",
		label: "SQLX prefetch",
		description:
			"Download the SQLX components that would otherwise be fetched during a first query or page, for example mysql, postgres, oracle, sqlserver, clickhouse, dameng or kingbase. Pass all to fetch every component; run sqlx prefetch --help for the complete list the CLI accepts.",
		parameters: Type.Object({
			components: Type.Array(Type.String(), {
				description:
					"Component names to download, for example mysql, postgres, oracle, sqlserver, clickhouse, dameng or kingbase; all fetches every component",
			}),
		}),
		annotations: { readOnlyHint: false, destructiveHint: false, openWorldHint: true },
		async execute(_toolCallId, params) {
			return result(await sqlx(["prefetch", ...params.components]));
		},
	});
}
