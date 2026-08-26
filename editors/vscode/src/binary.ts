import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { accessSync, constants, statSync } from "node:fs";
import { delimiter, isAbsolute, join, resolve, sep } from "node:path";

/** The name looked up on `PATH` when `ocomment.path` is unset. */
export const DEFAULT_COMMAND = "ocomment";

/** Everything the resolution depends on, so a test can supply all of it. */
export interface BinaryRequest {
	/** The `ocomment.path` setting, verbatim. */
	readonly configured: string;
	/** The first workspace folder, when one is open. */
	readonly workspaceRoot?: string | undefined;
	readonly env?: NodeJS.ProcessEnv | undefined;
	readonly platform?: NodeJS.Platform | undefined;
}

/** What resolution found, and why it failed when it did. */
export interface BinaryReport {
	/** The command as configured, after expansion. */
	readonly command: string;
	/** The file the command resolved to, when one exists. */
	readonly located?: string | undefined;
	/** The first line `--version` printed. */
	readonly version?: string | undefined;
	/** Why the binary cannot be used. */
	readonly error?: string | undefined;
}

function environment(request: BinaryRequest): NodeJS.ProcessEnv {
	return request.env ?? process.env;
}

function platformOf(request: BinaryRequest): NodeJS.Platform {
	return request.platform ?? process.platform;
}

function home(request: BinaryRequest): string | undefined {
	const env = environment(request);
	return platformOf(request) === "win32"
		? (env.USERPROFILE ?? env.HOME)
		: env.HOME;
}

function looksLikePath(value: string): boolean {
	return value.includes("/") || value.includes(sep);
}

/**
 * The command to spawn for a setting.
 *
 * A bare name is left alone so that `PATH` decides it; anything that looks
 * like a path is made absolute, because the server is spawned with the
 * workspace as its working directory only when there is one.
 */
export function commandFor(request: BinaryRequest): string {
	const configured = request.configured.trim();
	if (configured.length === 0) {
		return DEFAULT_COMMAND;
	}
	let expanded = configured;
	if (expanded === "~" || expanded.startsWith("~/") || expanded.startsWith("~\\")) {
		const directory = home(request);
		if (directory !== undefined) {
			expanded = join(directory, expanded.slice(1));
		}
	}
	if (!looksLikePath(expanded) || isAbsolute(expanded)) {
		return expanded;
	}
	return request.workspaceRoot === undefined
		? expanded
		: resolve(request.workspaceRoot, expanded);
}

function isRunnable(candidate: string, windows: boolean): boolean {
	try {
		if (!statSync(candidate).isFile()) {
			return false;
		}
		/* NOTE: Windows has no execute bit — every readable file is runnable
		 * there, and the extension list below is what decides instead — so
		 * asking for X_OK would turn every candidate down. */
		accessSync(candidate, windows ? constants.R_OK : constants.X_OK);
		return true;
	} catch {
		return false;
	}
}

/** The file a command resolves to, searching `PATH` for a bare name. */
export function locate(
	command: string,
	request: BinaryRequest,
): string | undefined {
	const windows = platformOf(request) === "win32";
	/* NOTE: `PATHEXT` is spelled in upper case and the files it names are
	 * almost always lower case. That only matters on a case-sensitive
	 * directory, which Windows has been able to mount since 1803, so each
	 * suffix is tried as given and folded down. */
	const suffixes = windows
		? [
				...new Set(
					["", ...(environment(request).PATHEXT ?? ".COM;.EXE;.BAT;.CMD").split(";")]
						.filter((suffix, index) => index === 0 || suffix.length > 0)
						.flatMap((suffix) => [suffix, suffix.toLowerCase()]),
				),
			]
		: [""];
	const directories = looksLikePath(command)
		? [undefined]
		: (environment(request).PATH ?? "").split(delimiter).filter(
				(entry) => entry.length > 0,
			);
	for (const directory of directories) {
		for (const suffix of suffixes) {
			const candidate =
				directory === undefined
					? `${command}${suffix}`
					: join(directory, `${command}${suffix}`);
			if (isRunnable(candidate, windows)) {
				return candidate;
			}
		}
	}
	return undefined;
}

/**
 * Resolve the binary and ask it for its version.
 *
 * Nothing is spawned when the file was not found, so a missing binary costs a
 * `stat` rather than a failed process, and the message names what was looked
 * for instead of repeating the operating system's `ENOENT`.
 *
 * The spawn is asynchronous on purpose: this runs during activation, and a
 * synchronous one would block the extension host for as long as the binary
 * takes to answer — up to the timeout, if it never does.
 */
export async function probe(request: BinaryRequest): Promise<BinaryReport> {
	const command = commandFor(request);
	const located = locate(command, request);
	if (located === undefined) {
		return {
			command,
			error: looksLikePath(command)
				? `${command} is not an executable file`
				: `${command} was not found on PATH`,
		};
	}
	try {
		const { stdout } = await version(located, request.workspaceRoot);
		return { command, located, version: stdout.split("\n")[0]?.trim() ?? "" };
	} catch (error) {
		return {
			command,
			located,
			error: error instanceof Error ? error.message : String(error),
		};
	}
}

const run = promisify(execFile);

function version(
	located: string,
	cwd: string | undefined,
): Promise<{ stdout: string }> {
	return run(located, ["--version"], {
		cwd,
		encoding: "utf8",
		timeout: 10_000,
		windowsHide: true,
	});
}
