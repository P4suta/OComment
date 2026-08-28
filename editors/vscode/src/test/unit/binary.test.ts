import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { DEFAULT_COMMAND, commandFor, locate, probe } from "../../binary";

function scratch(): string {
	return mkdtempSync(join(tmpdir(), "ocomment-binary-"));
}

test("an unset path means the ocomment on PATH", () => {
	assert.equal(commandFor({ configured: "" }), DEFAULT_COMMAND);
	assert.equal(commandFor({ configured: "   " }), DEFAULT_COMMAND);
	assert.equal(commandFor({ configured: DEFAULT_COMMAND }), DEFAULT_COMMAND);
});

test("a relative path is resolved against the workspace, an absolute one is not", () => {
	assert.equal(
		commandFor({ configured: "./bin/ocomment", workspaceRoot: "/w" }),
		join("/w", "bin", "ocomment"),
	);
	assert.equal(
		commandFor({ configured: "/opt/ocomment", workspaceRoot: "/w" }),
		"/opt/ocomment",
	);
	// NOTE: With no folder open there is nothing to resolve against, so the
	// NOTE: setting is handed to the spawn untouched rather than to the
	// NOTE: process working directory, which the user never chose.
	assert.equal(commandFor({ configured: "./bin/ocomment" }), "./bin/ocomment");
});

test("a leading tilde is expanded from the environment", () => {
	assert.equal(
		commandFor({
			configured: "~/bin/ocomment",
			env: { HOME: "/home/dev" },
			platform: "linux",
		}),
		join("/home/dev", "bin", "ocomment"),
	);
	assert.equal(
		commandFor({
			configured: "~/bin/ocomment",
			env: { USERPROFILE: "C:\\Users\\dev" },
			platform: "win32",
		}),
		join("C:\\Users\\dev", "bin", "ocomment"),
	);
	// NOTE: `~user` is a shell expansion this extension cannot resolve, so it
	// NOTE: stays literal instead of turning into a wrong path.
	assert.equal(
		commandFor({ configured: "~other/ocomment", env: { HOME: "/home/dev" } }),
		"~other/ocomment",
	);
});

test("a bare name is looked up on PATH and an unexecutable file is not a match", () => {
	const directory = scratch();
	const other = scratch();
	const executable = join(directory, "ocomment");
	writeFileSync(executable, "#!/bin/sh\n");
	chmodSync(executable, 0o755);
	const unreadable = join(other, "ocomment");
	writeFileSync(unreadable, "#!/bin/sh\n");
	chmodSync(unreadable, 0o644);

	assert.equal(
		locate(DEFAULT_COMMAND, {
			configured: "",
			env: { PATH: [other, directory].join(":") },
			platform: "linux",
		}),
		executable,
	);
	assert.equal(
		locate(DEFAULT_COMMAND, {
			configured: "",
			env: { PATH: other },
			platform: "linux",
		}),
		undefined,
	);
	assert.equal(
		locate(DEFAULT_COMMAND, { configured: "", env: {}, platform: "linux" }),
		undefined,
	);
});

test("PATHEXT decides the suffix on Windows", () => {
	const directory = scratch();
	const executable = join(directory, "ocomment.exe");
	// NOTE: Windows has no execute bit, so the mode is deliberately left plain
	// NOTE: here: finding this file is what proves the lookup does not ask for
	// NOTE: one on a platform that has none.
	writeFileSync(executable, "");
	assert.equal(
		locate(DEFAULT_COMMAND, {
			configured: "",
			env: { PATH: directory, PATHEXT: ".COM;.EXE;.BAT" },
			platform: "win32",
		}),
		executable,
	);
});

test("a path that names a file is used without consulting PATH", () => {
	const directory = scratch();
	const executable = join(directory, "tool");
	writeFileSync(executable, "#!/bin/sh\n");
	chmodSync(executable, 0o755);
	assert.equal(
		locate(executable, { configured: executable, env: {}, platform: "linux" }),
		executable,
	);
	assert.equal(
		locate(join(directory, "absent"), {
			configured: "",
			env: { PATH: directory },
			platform: "linux",
		}),
		undefined,
	);
});

test("probing reports the version of a real executable", async () => {
	// NOTE: `process.execPath --version` is the one executable every runner of
	// NOTE: this suite is guaranteed to have, so the probe is tested without
	// NOTE: depending on a built ocomment.
	const report = await probe({ configured: process.execPath });
	assert.equal(report.command, process.execPath);
	assert.equal(report.located, process.execPath);
	assert.equal(report.error, undefined);
	assert.match(String(report.version), /^v\d+\./u);
});

test("probing a binary that is not there reports why, and spawns nothing", async () => {
	const report = await probe({
		configured: "ocomment",
		env: { PATH: scratch() },
		platform: "linux",
	});
	assert.equal(report.command, "ocomment");
	assert.equal(report.located, undefined);
	assert.equal(report.version, undefined);
	assert.match(String(report.error), /PATH/u);
});
