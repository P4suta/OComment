import { resolve } from "node:path";

import { runTests } from "@vscode/test-electron";

async function main(): Promise<void> {
	const extensionDevelopmentPath = resolve(__dirname, "..", "..");
	const extensionTestsPath = resolve(__dirname, "suite", "index");
	const workspace = resolve(
		extensionDevelopmentPath,
		"test-fixtures",
		"workspace",
	);
	await runTests({
		extensionDevelopmentPath,
		extensionTestsPath,
		/* NOTE: The manifest declares no untrusted-workspace support, so the
		 * test instance has to be told to trust the fixture; without this the
		 * extension is loaded restricted and never activates. `--no-sandbox`
		 * is what lets Electron start as root inside a container. */
		launchArgs: [
			workspace,
			"--disable-extensions",
			"--disable-workspace-trust",
			"--disable-gpu",
			"--no-sandbox",
		],
	});
}

main().catch((error: unknown) => {
	console.error(error);
	process.exitCode = 1;
});
