import * as vscode from "vscode";
import {
	LanguageClient,
	type LanguageClientOptions,
	RevealOutputChannelOn,
	type ServerOptions,
	State,
} from "vscode-languageclient/node";

import { probe } from "./binary";
import { Serial } from "./serial";
import { CommentStatus, type ServerState } from "./status";

const SECTION = "ocomment";
const QUICK_START = "https://github.com/P4suta/OComment#quick-start";

/** The extension's whole runtime: one server, one status bar entry. */
class OComment {
	private readonly output: vscode.LogOutputChannel;
	private readonly item: vscode.StatusBarItem;
	private readonly status: CommentStatus;
	private readonly watcher: vscode.FileSystemWatcher;
	private readonly disposables: vscode.Disposable[] = [];
	/* NOTE: Every start and stop goes through this, so a settings change during a
	 * restart cannot leave a second server running with nothing holding it. */
	private readonly lifecycle = new Serial();
	private client: LanguageClient | undefined;
	private clientState: vscode.Disposable | undefined;
	private state: ServerState = "stopped";

	constructor(context: vscode.ExtensionContext) {
		this.output = vscode.window.createOutputChannel("OComment", {
			log: true,
		});
		this.item = vscode.window.createStatusBarItem(
			vscode.StatusBarAlignment.Right,
			0,
		);
		this.status = new CommentStatus(this.item);
		/* NOTE: One watcher for the life of the extension. It was created per start
		 * before, which leaked one file watcher for every restart: the client
		 * only ever disposes the listeners it hooked onto the watcher it was
		 * handed, never the watcher itself, which is what makes handing the
		 * same one to each successive client safe. */
		this.watcher = vscode.workspace.createFileSystemWatcher(
			"**/.ocomment.{toml,lock}",
		);
		this.disposables.push(this.output, this.item, this.watcher);

		this.register("ocomment.fixActiveDocument", () => this.fixActiveDocument());
		this.register("ocomment.restartServer", () => this.restart());
		this.register("ocomment.showOutput", () => {
			this.output.show(true);
		});
		/* NOTE: `ocomment.fixWorkspace` is deliberately not registered here. The
		 * language client registers a handler for every name the server lists
		 * in `executeCommandProvider`, and that registration throws if the name
		 * is already taken — which would take the whole client down with it.
		 * The manifest contributes the name for its palette title only. */

		this.disposables.push(
			vscode.languages.onDidChangeDiagnostics(() => {
				this.refresh();
			}),
			vscode.workspace.onDidChangeConfiguration((event) => {
				if (event.affectsConfiguration(SECTION)) {
					void this.restart();
				}
			}),
		);
		context.subscriptions.push(...this.disposables);
	}

	private register(name: string, handler: () => unknown): void {
		this.disposables.push(
			vscode.commands.registerCommand(name, () => handler()),
		);
	}

	private configuration(): vscode.WorkspaceConfiguration {
		return vscode.workspace.getConfiguration(SECTION);
	}

	private workspaceRoot(): string | undefined {
		const folder = vscode.workspace.workspaceFolders?.[0];
		return folder?.uri.scheme === "file" ? folder.uri.fsPath : undefined;
	}

	private refresh(): void {
		this.status.update(this.state, vscode.languages.getDiagnostics());
	}

	private enter(state: ServerState): void {
		this.state = state;
		this.refresh();
	}

	/** Stop whatever is running, then start the server, one request at a time. */
	async start(): Promise<void> {
		/* INVARIANT: `launch` and `shutdown` are the bodies, and neither may reach for
		 * the queue itself: work queued from inside the queue waits for the
		 * work that queued it, which never finishes. Every public entry
		 * point queues exactly once, here or in `stop`. */
		return this.lifecycle.run(async () => {
			await this.shutdown();
			await this.launch();
		});
	}

	/** Resolve the binary, then start the server against it. */
	private async launch(): Promise<void> {
		const configuration = this.configuration();
		if (!configuration.get<boolean>("enable", true)) {
			this.output.info("ocomment.enable is off; the server is stopped.");
			this.enter("disabled");
			return;
		}

		const workspaceRoot = this.workspaceRoot();
		const report = await probe({
			configured: configuration.get<string>("path", ""),
			workspaceRoot,
		});
		if (report.error !== undefined || report.located === undefined) {
			this.output.error(`Cannot start OComment: ${String(report.error)}`);
			this.enter("unavailable");
			this.announceMissing(String(report.error));
			return;
		}
		this.output.info(`Using ${report.located} (${String(report.version)})`);

		/* NOTE: No transport is named on purpose. The client already talks over
		 * the child's stdio when none is given; naming the stdio transport
		 * additionally appends `--stdio` to the arguments, and `ocomment lsp`
		 * rejects an argument it does not define rather than ignoring it, so the
		 * server would exit before the first request reached it. */
		const serverOptions: ServerOptions = {
			command: report.located,
			args: ["lsp", ...configuration.get<string[]>("extraArgs", [])],
			options: workspaceRoot === undefined ? {} : { cwd: workspaceRoot },
		};
		const languages = configuration.get<string[]>("languages", []);
		const clientOptions: LanguageClientOptions = {
			documentSelector: languages.map((language) => ({
				scheme: "file",
				language,
			})),
			synchronize: {
				/* NOTE: The server registers watchers for these two names itself
				 * when the client advertises dynamic registration. This one is
				 * the fallback for the same files, so a configuration change is
				 * picked up either way. */
				fileEvents: this.watcher,
			},
			/* NOTE: The server advertises `workspaceDiagnostics`, and the client
			 * drives that pull on its own; the two flags below are the ones for
			 * the open documents, which the client would otherwise only pull on
			 * open. */
			diagnosticPullOptions: { onChange: true, onSave: true },
			outputChannel: this.output,
			revealOutputChannelOn: RevealOutputChannelOn.Never,
		};

		const client = new LanguageClient(
			"ocomment",
			"OComment",
			serverOptions,
			clientOptions,
		);
		this.client = client;
		// NOTE: Held on its own rather than in `disposables`, which lives as
		// NOTE: long as the extension does: a restart replaces the client, and
		// NOTE: a listener per restart would accumulate for the session.
		this.clientState = client.onDidChangeState((event) => {
			this.enter(event.newState === State.Running ? "running" : "stopped");
		});
		this.enter("starting");
		try {
			await client.start();
			this.enter("running");
		} catch (error) {
			this.output.error(`OComment failed to start: ${String(error)}`);
			this.client = undefined;
			this.enter("stopped");
		}
	}

	async stop(): Promise<void> {
		return this.lifecycle.run(() => this.shutdown());
	}

	/** Take down the client, if there is one, and forget it. */
	private async shutdown(): Promise<void> {
		const client = this.client;
		this.client = undefined;
		this.clientState?.dispose();
		this.clientState = undefined;
		if (client !== undefined) {
			await client.stop().catch((error: unknown) => {
				this.output.error(`OComment did not stop cleanly: ${String(error)}`);
			});
		}
		this.enter("stopped");
	}

	async restart(): Promise<void> {
		await this.start();
	}

	private announceMissing(reason: string): void {
		const install = "Install OComment";
		const settings = "Open settings";
		void vscode.window
			.showWarningMessage(`OComment: ${reason}`, install, settings)
			.then(async (choice) => {
				if (choice === install) {
					await vscode.env.openExternal(vscode.Uri.parse(QUICK_START));
				} else if (choice === settings) {
					await vscode.commands.executeCommand(
						"workbench.action.openSettings",
						"ocomment.path",
					);
				}
			});
	}

	private async fixActiveDocument(): Promise<void> {
		const editor = vscode.window.activeTextEditor;
		if (editor === undefined) {
			void vscode.window.showWarningMessage(
				"OComment: open a file before removing its comments.",
			);
			return;
		}
		if (this.client === undefined || this.state !== "running") {
			void vscode.window.showWarningMessage(
				"OComment: the server is not running.",
			);
			return;
		}
		/* NOTE: `ocomment.fixDocument` is the server's own command, registered
		 * by the language client. Going through it rather than through a second
		 * request keeps this command and the code lens on one code path, and
		 * the edit arrives as the annotated workspace edit the server built. */
		await vscode.commands.executeCommand(
			"ocomment.fixDocument",
			editor.document.uri.toString(),
		);
	}

	async dispose(): Promise<void> {
		await this.stop();
		for (const disposable of this.disposables.splice(0)) {
			disposable.dispose();
		}
	}
}

let extension: OComment | undefined;

export async function activate(
	context: vscode.ExtensionContext,
): Promise<void> {
	extension = new OComment(context);
	await extension.start();
}

export async function deactivate(): Promise<void> {
	const current = extension;
	extension = undefined;
	await current?.dispose();
}
