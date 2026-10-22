import assert from "node:assert/strict";
import { chmodSync, mkdtempSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
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
	const root = join(tmpdir(), "workspace");
	const absolute = join(tmpdir(), "elsewhere", "ocomment");
	assert.equal(
		commandFor({ configured: "./bin/ocomment", workspaceRoot: root }),
		join(root, "bin", "ocomment"),
	);
	assert.equal(
		commandFor({ configured: absolute, workspaceRoot: root }),
		absolute,
	);
	// NOTE: With no folder open there is nothing to resolve against, so the setting is handed to the spawn untouched rather than to the process working directory, which the user never chose.
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
	// NOTE: `~user` is a shell expansion this extension cannot resolve, so it stays literal instead of turning into a wrong path.
	assert.equal(
		commandFor({ configured: "~other/ocomment", env: { HOME: "/home/dev" } }),
		"~other/ocomment",
	);
});

test("PATH lookup follows native file permissions and candidate order", () => {
	const directory = scratch();
	const other = scratch();
	const name = process.platform === "win32" ? "ocomment.exe" : "ocomment";
	const executable = join(directory, name);
	writeFileSync(executable, "#!/bin/sh\n");
	chmodSync(executable, 0o755);
	const unreadable = join(other, name);
	writeFileSync(unreadable, "#!/bin/sh\n");
	chmodSync(unreadable, 0o644);

	assert.equal(
		locate(DEFAULT_COMMAND, {
			configured: "",
			env: { PATH: [other, directory].join(delimiter), PATHEXT: ".exe" },
			platform: process.platform,
		}),
		process.platform === "win32" ? unreadable : executable,
	);
	assert.equal(
		locate(DEFAULT_COMMAND, {
			configured: "",
			env: { PATH: other, PATHEXT: ".exe" },
			platform: process.platform,
		}),
		process.platform === "win32" ? unreadable : undefined,
	);
	assert.equal(
		locate(DEFAULT_COMMAND, {
			configured: "",
			env: { PATH: [scratch(), directory].join(delimiter), PATHEXT: ".exe" },
			platform: process.platform,
		}),
		executable,
	);
	assert.equal(
		locate(DEFAULT_COMMAND, { configured: "", env: {}, platform: "linux" }),
		undefined,
	);
});

test("PATHEXT decides the suffix on Windows", () => {
	const directory = scratch();
	const executable = join(directory, "ocomment.exe");
	// NOTE: Windows has no execute bit, so the mode is deliberately left plain here: finding this file is what proves the lookup does not ask for one on a platform that has none.
	writeFileSync(executable, "");
	const located = locate(DEFAULT_COMMAND, {
		configured: "",
		env: { PATH: directory, PATHEXT: ".COM;.EXE;.BAT" },
		platform: "win32",
	});
	assert.ok(located);
	const actual = statSync(located);
	const expected = statSync(executable);
	assert.equal(actual.dev, expected.dev);
	assert.equal(actual.ino, expected.ino);
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
	// NOTE: `process.execPath --version` is the one executable every runner of this suite is guaranteed to have, so the probe is tested without depending on a built ocomment.
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
