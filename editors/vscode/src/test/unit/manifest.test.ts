import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const extensionRoot = join(__dirname, "..", "..", "..");
const repositoryRoot = join(extensionRoot, "..", "..");

interface Manifest {
	version: string;
	engines: { vscode: string };
	main: string;
	activationEvents: string[];
	contributes: {
		configuration: {
			properties: Record<string, { default?: unknown }>;
		};
		commands: { command: string; title: string }[];
	};
}

const manifest = JSON.parse(
	readFileSync(join(extensionRoot, "package.json"), "utf8"),
) as Manifest;

test("the extension is versioned with the crate it launches", () => {
	// NOTE: `publish-vscode` refuses to publish a Marketplace version that is
	// NOTE: not the release tag, so a bump that forgot this file would fail the
	// NOTE: release rather than ship a mismatched pair. Catching it here means
	// NOTE: the bump is caught on the pull request instead.
	const workspace = readFileSync(
		join(repositoryRoot, "rust", "Cargo.toml"),
		"utf8",
	);
	const section = workspace.slice(workspace.indexOf("[workspace.package]"));
	const version = /^version\s*=\s*"([^"]+)"/mu.exec(section);
	assert.ok(version, "rust/Cargo.toml has no [workspace.package] version");
	assert.equal(manifest.version, version[1]);
});

test("every language the extension attaches to also activates it", () => {
	const activated = manifest.activationEvents
		.filter((event) => event.startsWith("onLanguage:"))
		.map((event) => event.slice("onLanguage:".length));
	const configured = manifest.contributes.configuration.properties[
		"ocomment.languages"
	].default as string[];
	assert.deepEqual([...activated].sort(), [...configured].sort());
	assert.equal(configured.length, 20);
	// NOTE: A workspace can hold a configuration file and no open editor, and
	// NOTE: the status bar and the workspace fix have to work there too.
	assert.ok(
		manifest.activationEvents.includes("workspaceContains:**/.ocomment.toml"),
	);
});

test("every contributed command is titled for the palette", () => {
	const titles = new Map(
		manifest.contributes.commands.map((entry) => [entry.command, entry.title]),
	);
	assert.deepEqual(
		[...titles.keys()].sort(),
		[
			"ocomment.fixActiveDocument",
			"ocomment.fixWorkspace",
			"ocomment.restartServer",
			"ocomment.showOutput",
		],
	);
	assert.equal(
		titles.get("ocomment.fixActiveDocument"),
		"OComment: Remove comments in file",
	);
	for (const title of titles.values()) {
		assert.match(title, /^OComment: /u);
	}
});
