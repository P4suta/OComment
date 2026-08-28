import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";

const extensionRoot = join(__dirname, "..", "..", "..");

interface Manifest {
	version: string;
	engines: { vscode: string };
	main: string;
	scripts: Record<string, string>;
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

test("the bundled extension is packaged without a second dependency tree", () => {
	assert.equal(manifest.main, "./dist/extension.js");
	assert.match(manifest.scripts.package, /(?:^|\s)--no-dependencies(?:\s|$)/u);
});

test("the extension carries its own semantic version", () => {
	assert.match(manifest.version, /^\d+\.\d+\.\d+$/u);
});

test("every language the extension attaches to also activates it", () => {
	const activated = manifest.activationEvents
		.filter((event) => event.startsWith("onLanguage:"))
		.map((event) => event.slice("onLanguage:".length));
	const configured = manifest.contributes.configuration.properties[
		"ocomment.languages"
	].default as string[];
	assert.deepEqual([...activated].sort(), [...configured].sort());
	// NOTE: The literal is the count, so dropping an identifier fails here rather
	// NOTE: than shrinking the set the extension attaches to in silence. Every
	// NOTE: written-out count of it -- the extension description, the README,
	// NOTE: docs/editors.md, both changelogs -- is checked against this same list
	// NOTE: by `every_written_language_count_matches_what_it_counts` in
	// NOTE: rust/ocomment/tests/spec_languages.rs.
	assert.equal(configured.length, 35);
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
