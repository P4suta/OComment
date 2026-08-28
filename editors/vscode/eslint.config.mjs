import js from "@eslint/js";
import tseslint from "typescript-eslint";

export default tseslint.config(
	// NOTE: `.vscode-test` holds a whole downloaded VS Code, so leaving it in
	// NOTE: would hand the type-aware rules a gigabyte of bundled JavaScript and
	// NOTE: run the linter out of heap.
	{
		ignores: [
			".vscode-test/**",
			"dist/**",
			"node_modules/**",
			"out/**",
			"*.vsix",
		],
	},
	js.configs.recommended,
	tseslint.configs.recommendedTypeChecked,
	tseslint.configs.stylisticTypeChecked,
	{
		languageOptions: {
			parserOptions: {
				projectService: true,
				tsconfigRootDir: import.meta.dirname,
			},
		},
		rules: {
			"@typescript-eslint/restrict-template-expressions": [
				"error",
				{ allowNumber: true },
			],
		},
	},
	{
		// NOTE: `node:test` is meant to be called without awaiting at the top
		// NOTE: level of a file: the runner collects the cases and reports them.
		files: ["src/test/**/*.test.ts"],
		rules: { "@typescript-eslint/no-floating-promises": "off" },
	},
	{
		// NOTE: The two build scripts are plain ES modules outside tsconfig's
		// NOTE: `include`, so the type-aware rules have no program for them.
		files: ["*.mjs"],
		extends: [tseslint.configs.disableTypeChecked],
		languageOptions: {
			globals: { console: "readonly", process: "readonly" },
		},
	},
);
