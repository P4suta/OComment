/**
 * A registry-backed runner for the tests that need a real extension host.
 *
 * The unit suites run under `node --test`; this one cannot, because it is
 * loaded inside VS Code's own process, so the runner is here rather than in a
 * dependency.
 */

interface Case {
	readonly name: string;
	readonly body: () => void | Promise<void>;
}

const cases: Case[] = [];

/** Register one case. Cases run in registration order. */
export function test(name: string, body: () => void | Promise<void>): void {
	cases.push({ name, body });
}

/** Poll until `condition` holds, or fail with `message`. */
export async function waitFor(
	condition: () => boolean | Promise<boolean>,
	message: string,
	timeoutMs = 60_000,
): Promise<void> {
	const deadline = Date.now() + timeoutMs;
	for (;;) {
		if (await condition()) {
			return;
		}
		if (Date.now() >= deadline) {
			throw new Error(`timed out after ${timeoutMs} ms: ${message}`);
		}
		await new Promise((resolve) => setTimeout(resolve, 100));
	}
}

/** Run every registered case, then throw a report if any of them failed. */
export async function runRegistered(): Promise<void> {
	const failures: string[] = [];
	console.log(`1..${cases.length}`);
	for (const [index, entry] of cases.entries()) {
		try {
			await entry.body();
			console.log(`ok ${index + 1} - ${entry.name}`);
		} catch (error) {
			console.log(`not ok ${index + 1} - ${entry.name}`);
			const detail =
				error instanceof Error ? (error.stack ?? error.message) : String(error);
			console.log(detail.replace(/^/gmu, "  # "));
			failures.push(entry.name);
		}
	}
	if (failures.length > 0) {
		throw new Error(
			`${failures.length} of ${cases.length} failed: ${failures.join(", ")}`,
		);
	}
}
