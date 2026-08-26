/**
 * One thing at a time, in the order it was asked for.
 *
 * Starting the language server is asynchronous from the moment the binary is
 * probed to the moment the client is running, and VS Code will happily ask for
 * another one in the middle of it: a settings sync rewrites `ocomment.path`
 * while `OComment: Restart server` is still resolving, and both requests are
 * live at once. Each one stops "the" client and starts a new one, so two
 * overlapping requests can leave a `ocomment lsp` process behind with nothing
 * holding it — the second start overwrites the field the first would have
 * stopped.
 *
 * Declaring it here rather than in `extension.ts` keeps `vscode` out of this
 * file's imports, so the unit suite can exercise the ordering without an
 * extension host, exactly as `status.ts` does for the status bar entry.
 */
export class Serial {
	private tail: Promise<void> = Promise.resolve();

	/**
	 * Queue `work` behind everything already queued, and answer with what it
	 * settles to.
	 *
	 * The answer is the caller's own: a failed start rejects for whoever asked
	 * for it and for nobody else.
	 */
	run(work: () => Promise<void>): Promise<void> {
		const started = this.tail.then(work);
		/* NOTE: The chain remembers only that the previous work finished, never how.
		 * Chaining the rejection instead would fail every later request with
		 * the first failure — and, because nothing awaits the field itself,
		 * would surface as an unhandled rejection as well. */
		this.tail = started.then(
			() => undefined,
			() => undefined,
		);
		return started;
	}
}
