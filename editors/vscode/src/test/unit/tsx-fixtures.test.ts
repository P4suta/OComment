import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import test from "node:test";
import * as ts from "typescript";

test("escaped type parameter names remain valid TypeScript syntax", (): void => {
	const source =
		'const pick = <T\\u{000000065} extends string>(value: Te): Te => value; const view = <div>{pick("a")}</div>;';
	const result = ts.transpileModule(source, {
		fileName: "escaped.tsx",
		reportDiagnostics: true,
		compilerOptions: {
			jsx: ts.JsxEmit.Preserve,
			target: ts.ScriptTarget.ES2022,
			module: ts.ModuleKind.ESNext,
		},
	});
	assert.ok(result.diagnostics);
	assert.equal(
		result.diagnostics.filter(
			(diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error,
		).length,
		0,
	);
});

test("the compiler diagnoses an ambiguous extends head without arrow parameters", (): void => {
	const source =
		"const view = <View extends Other>text // literal\n{1 /* inside */}</View>;";
	const result = ts.transpileModule(source, {
		fileName: "ambiguous.tsx",
		reportDiagnostics: true,
		compilerOptions: {
			jsx: ts.JsxEmit.Preserve,
			target: ts.ScriptTarget.ES2022,
			module: ts.ModuleKind.ESNext,
		},
	});
	assert.ok(result.diagnostics);
	assert.ok(
		result.diagnostics.some(
			(diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error,
		),
	);
});

test("generic-arrow fixtures agree with the pinned TypeScript parser", (): void => {
	const extensionRoot = join(__dirname, "..", "..", "..");
	const fixturePath = join(
		extensionRoot,
		"..",
		"..",
		"spec",
		"fixtures",
		"v1",
		"hazards.json",
	);
	const corpus: unknown = JSON.parse(readFileSync(fixturePath, "utf8"));
	assert.ok(typeof corpus === "object" && corpus !== null && "cases" in corpus);
	assert.ok(Array.isArray(corpus.cases));
	const cases: readonly unknown[] = corpus.cases;
	let checked = 0;
	let valid = 0;
	let invalid = 0;
	for (const entry of cases) {
		assert.ok(typeof entry === "object" && entry !== null && "id" in entry);
		assert.equal(typeof entry.id, "string");
		if (typeof entry.id !== "string" || !entry.id.startsWith("tsx-generic-")) {
			continue;
		}
		assert.ok("language" in entry && "dialect" in entry);
		assert.ok("source_utf8" in entry && typeof entry.source_utf8 === "string");
		assert.ok(
			"expect" in entry &&
				typeof entry.expect === "object" &&
				entry.expect !== null,
		);
		assert.ok(
			"valid" in entry.expect && typeof entry.expect.valid === "boolean",
		);
		const tsx = entry.language === "typescript" && entry.dialect === "tsx";
		const plain =
			entry.language === "typescript" && entry.dialect === "standard";
		const jsx = entry.language === "javascript" && entry.dialect === "jsx";
		assert.ok(tsx || plain || jsx, `${entry.id}: unsupported fixture language`);
		const suffix = tsx ? "tsx" : jsx ? "jsx" : "ts";
		const result = ts.transpileModule(entry.source_utf8, {
			fileName: `${entry.id}.${suffix}`,
			reportDiagnostics: true,
			compilerOptions: {
				jsx: ts.JsxEmit.Preserve,
				target: ts.ScriptTarget.ES2022,
				module: ts.ModuleKind.ESNext,
			},
		});
		assert.ok(result.diagnostics, "the parser must supply diagnostic output");
		const errors = result.diagnostics.filter(
			(diagnostic) => diagnostic.category === ts.DiagnosticCategory.Error,
		);
		const detail = errors
			.map((diagnostic): string =>
				ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"),
			)
			.join("\n");
		assert.equal(
			errors.length === 0,
			entry.expect.valid,
			`${entry.id}: ${detail}`,
		);
		checked += 1;
		if (entry.expect.valid) {
			valid += 1;
		} else {
			invalid += 1;
		}
	}
	assert.ok(checked >= 13, "the compiler must reach every targeted regression");
	assert.ok(valid > 0, "the oracle must accept a valid source");
	assert.ok(invalid > 0, "the oracle must reject broken syntax");
});
