import assert from "node:assert/strict";
import test from "node:test";

import {
	CommentStatus,
	DIAGNOSTIC_SOURCE,
	type DiagnosticEntry,
	type StatusItem,
	countOwned,
} from "../../status";

function fake(): StatusItem & { visible: boolean } {
	return {
		text: "",
		tooltip: undefined,
		command: undefined,
		visible: false,
		show(): void {
			this.visible = true;
		},
		hide(): void {
			this.visible = false;
		},
	};
}

const entries: DiagnosticEntry[] = [
	[
		"file:///a.rs",
		[
			{ source: DIAGNOSTIC_SOURCE },
			{ source: "rust-analyzer" },
			{ source: DIAGNOSTIC_SOURCE },
		],
	],
	["file:///b.py", [{ source: undefined }, { source: DIAGNOSTIC_SOURCE }]],
	["file:///c.go", [{ source: "gopls" }]],
];

test("only this server's diagnostics are counted", () => {
	assert.equal(countOwned(entries), 3);
	assert.equal(countOwned([]), 0);
	assert.equal(countOwned([["file:///d.rs", []]]), 0);
});

test("a running server shows the count and opens the output channel", () => {
	const item = fake();
	new CommentStatus(item).update("running", entries);
	assert.equal(item.text, "$(comment) 3");
	assert.equal(item.command, "ocomment.showOutput");
	assert.match(String(item.tooltip), /3 removable comments/u);
	assert.equal(item.visible, true);
});

test("one removable comment is not reported in the plural", () => {
	const item = fake();
	new CommentStatus(item).update("running", [
		["file:///a.rs", [{ source: DIAGNOSTIC_SOURCE }]],
	]);
	assert.equal(item.text, "$(comment) 1");
	assert.match(String(item.tooltip), /1 removable comment[^s]/u);
});

test("a clean workspace still shows the item, so the output stays reachable", () => {
	const item = fake();
	new CommentStatus(item).update("running", []);
	assert.equal(item.text, "$(comment) 0");
	assert.equal(item.visible, true);
});

test("a missing binary is reported instead of a count", () => {
	const item = fake();
	new CommentStatus(item).update("unavailable", entries);
	assert.equal(item.text, "$(warning) OComment");
	assert.match(String(item.tooltip), /not found/u);
	assert.equal(item.visible, true);
});

test("a stopped server says so rather than showing a stale count", () => {
	const item = fake();
	const status = new CommentStatus(item);
	status.update("running", entries);
	status.update("stopped", entries);
	assert.equal(item.text, "$(circle-slash) OComment");
	assert.equal(item.visible, true);
});

test("turning the extension off hides the item", () => {
	const item = fake();
	const status = new CommentStatus(item);
	status.update("running", entries);
	status.update("disabled", entries);
	assert.equal(item.visible, false);
});
