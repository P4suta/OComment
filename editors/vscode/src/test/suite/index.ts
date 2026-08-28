import { runRegistered } from "../harness";

import "./extension.test";

/** The entry point `@vscode/test-electron` calls inside the extension host. */
export function run(): Promise<void> {
	return runRegistered();
}
