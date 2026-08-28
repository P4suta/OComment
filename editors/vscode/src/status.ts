/** The `source` the OComment language server stamps on every diagnostic. */
export const DIAGNOSTIC_SOURCE = "ocomment";

/** What the status bar is reporting about the server. */
export type ServerState =
	| "running"
	| "starting"
	| "stopped"
	| "unavailable"
	| "disabled";

/** The only part of a diagnostic the count depends on. */
export interface SourcedDiagnostic {
	readonly source?: string | undefined;
}

/** One entry of `vscode.languages.getDiagnostics()`. */
export type DiagnosticEntry = readonly [unknown, readonly SourcedDiagnostic[]];

/**
 * The part of `vscode.StatusBarItem` this module drives.
 *
 * Declaring it structurally keeps `vscode` out of this file's imports, so the
 * unit suite can exercise the item without an extension host.
 */
export interface StatusItem {
	text: string;
	tooltip?: unknown;
	command?: unknown;
	show(): void;
	hide(): void;
}

/** How many of the workspace's diagnostics this server produced. */
export function countOwned(entries: readonly DiagnosticEntry[]): number {
	let total = 0;
	for (const [, diagnostics] of entries) {
		for (const diagnostic of diagnostics) {
			if (diagnostic.source === DIAGNOSTIC_SOURCE) {
				total += 1;
			}
		}
	}
	return total;
}

/** The status bar entry for the OComment server. */
export class CommentStatus {
	private readonly item: StatusItem;

	constructor(item: StatusItem) {
		this.item = item;
	}

	/** Redraw the entry for a server state and the current diagnostics. */
	update(state: ServerState, entries: readonly DiagnosticEntry[]): void {
		if (state === "disabled") {
			this.item.hide();
			return;
		}
		this.item.command = "ocomment.showOutput";
		switch (state) {
			case "starting":
				this.item.text = "$(loading~spin) OComment";
				this.item.tooltip = "OComment is starting.";
				break;
			case "stopped":
				this.item.text = "$(circle-slash) OComment";
				this.item.tooltip =
					"The OComment server is not running. Run “OComment: Restart server” to start it again.";
				break;
			case "unavailable":
				this.item.text = "$(warning) OComment";
				this.item.tooltip =
					"The ocomment executable was not found. Install it, or set ocomment.path.";
				break;
			case "running": {
				const count = countOwned(entries);
				this.item.text = `$(comment) ${String(count)}`;
				this.item.tooltip = `${String(count)} removable ${
					count === 1 ? "comment" : "comments"
				} in the open files. Click to show the OComment output.`;
				break;
			}
		}
		this.item.show();
	}
}
