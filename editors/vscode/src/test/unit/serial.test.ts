import assert from "node:assert/strict";
import test from "node:test";

import { Serial } from "../../serial";

/** Yield to the event loop, so a body really is interruptible. */
function tick(): Promise<void> {
	return new Promise((resolve) => setTimeout(resolve, 0));
}

test("queued work runs one at a time, in the order it was queued", async () => {
	const serial = new Serial();
	const trace: string[] = [];
	const body = async (name: string): Promise<void> => {
		trace.push(`${name} in`);
		await tick();
		trace.push(`${name} out`);
	};

	await Promise.all([
		serial.run(() => body("first")),
		serial.run(() => body("second")),
	]);

	assert.deepEqual(trace, [
		"first in",
		"first out",
		"second in",
		"second out",
	]);
});

test("a concurrent restart cannot leave two servers running", async () => {
	const serial = new Serial();
	let live = 0;
	let peak = 0;
	// NOTE: The shape of `start()`: stop whatever is there, then bring one up,
	// NOTE: with an await either side. Without the queue the three requests
	// NOTE: below interleave and `peak` reaches 3.
	const restart = (): Promise<void> =>
		serial.run(async () => {
			live = 0;
			await tick();
			live += 1;
			peak = Math.max(peak, live);
			await tick();
		});

	await Promise.all([restart(), restart(), restart()]);

	assert.equal(peak, 1);
	assert.equal(live, 1);
});

test("a failed start rejects for its own caller and holds nothing else up", async () => {
	const serial = new Serial();
	const trace: string[] = [];

	const failed = serial.run(() => {
		trace.push("failed");
		return Promise.reject(new Error("no binary"));
	});
	const next = serial.run(async () => {
		trace.push("next");
		await tick();
	});

	await assert.rejects(failed, /no binary/u);
	await next;
	assert.deepEqual(trace, ["failed", "next"]);
});

test("work queued after the chain has drained still runs", async () => {
	const serial = new Serial();
	let runs = 0;
	const body = async (): Promise<void> => {
		runs += 1;
		await tick();
	};

	await serial.run(body);
	await serial.run(body);

	assert.equal(runs, 2);
});
