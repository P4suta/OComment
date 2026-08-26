import assert from "node:assert/strict";

import * as vscode from "vscode";

import { DIAGNOSTIC_SOURCE } from "../../status";
import { test, waitFor } from "../harness";

const EXTENSION_ID = "P4suta.ocomment";

function fixture(name: string): vscode.Uri {
	const folder = vscode.workspace.workspaceFolders?.[0];
	assert.ok(folder, "the test instance opened no workspace folder");
	return vscode.Uri.joinPath(folder.uri, name);
}

function owned(uri: vscode.Uri): vscode.Diagnostic[] {
	return vscode.languages
		.getDiagnostics(uri)
		.filter((diagnostic) => diagnostic.source === DIAGNOSTIC_SOURCE);
}

test("the workspace's .ocomment.toml activates the extension", async () => {
	const extension = vscode.extensions.getExtension(EXTENSION_ID);
	assert.ok(extension, `${EXTENSION_ID} was not loaded`);
	await extension.activate();
	assert.equal(extension.isActive, true);
});

test("the language client registers the server's workspace fix", async () => {
	// NOTE: `ocomment.fixWorkspace` is contributed for its palette title only.
	// NOTE: The handler is the one the language client registers out of the
	// NOTE: server's `executeCommandProvider`, so seeing the command here is
	// NOTE: what proves the server started and finished initialising.
	await waitFor(
		async () =>
			(await vscode.commands.getCommands(true)).includes("ocomment.fixWorkspace"),
		"the server never registered ocomment.fixWorkspace",
	);
	const commands = await vscode.commands.getCommands(true);
	assert.ok(commands.includes("ocomment.fixActiveDocument"));
	assert.ok(commands.includes("ocomment.restartServer"));
	assert.ok(commands.includes("ocomment.showOutput"));
});

test("a removable comment is reported as an ocomment diagnostic", async () => {
	const uri = fixture("sample.rs");
	const document = await vscode.workspace.openTextDocument(uri);
	await vscode.window.showTextDocument(document);
	await waitFor(
		() => owned(uri).length === 1,
		`sample.rs never produced exactly one ${DIAGNOSTIC_SOURCE} diagnostic`,
	);
	const [diagnostic] = owned(uri);
	assert.ok(diagnostic);
	assert.equal(diagnostic.severity, vscode.DiagnosticSeverity.Hint);
	assert.equal(
		document.getText(diagnostic.range).trim(),
		"// the extension test removes this",
	);
});

test("fixActiveDocument removes the comment it reported", async () => {
	const uri = fixture("sample.rs");
	const document = await vscode.workspace.openTextDocument(uri);
	const editor = await vscode.window.showTextDocument(document);
	assert.equal(editor.document.uri.toString(), uri.toString());
	assert.ok(document.getText().includes("// the extension test removes this"));

	await vscode.commands.executeCommand("ocomment.fixActiveDocument");
	await waitFor(
		() => !document.getText().includes("// the extension test removes this"),
		"fixActiveDocument left the comment in the buffer",
	);
	// NOTE: The removal is byte-preserving, so the code either side of the
	// NOTE: comment has to come back untouched, and the file on disk is not
	// NOTE: written: the edit is reverted below so the fixture stays as it is
	// NOTE: in the repository.
	assert.ok(document.getText().includes("let value = 1;"));
	assert.ok(document.isDirty);
	await vscode.commands.executeCommand("workbench.action.files.revert");
	await waitFor(
		() => !document.isDirty,
		"the fixture buffer was left dirty for the next run",
	);
});
