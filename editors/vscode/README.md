# OComment for VS Code

Fast, byte-preserving comment checking and removal for Rust, OCaml, C, C++,
Go, Java, JavaScript, TypeScript, Python, Shell, HTML, CSS, JSONC, SQL,
Kotlin, TOML, Lua, YAML, PHP, Ruby, Zig, R, Dart, and Swift — including the
JSX/TSX,
Objective-C/C++, and CUDA dialects.

Removable comments appear as hints. Removing them is a code action, never a
formatting pass, so nothing happens to a file until you ask for it.

## Install the `ocomment` binary first

This extension is a client for the `ocomment` language server; it does not
bundle one. Install the binary and make sure it is on your `PATH`:

```sh
cargo install ocomment --locked
# or: docker pull ghcr.io/p4suta/ocomment
```

Release archives for Linux, macOS, and Windows are attached to every
[GitHub release](https://github.com/P4suta/OComment/releases). If the binary
lives somewhere else, point `ocomment.path` at it.

## Commands

| Command | What it does |
| --- | --- |
| `OComment: Remove comments in file` | Applies every removal the server offers for the active editor. |
| `OComment: Remove comments in workspace` | The same across every file the server has scanned. |
| `OComment: Restart server` | Restarts the language server. |
| `OComment: Show output` | Opens the OComment output channel. |

The status bar shows how many removable comments the open files hold; clicking
it opens the output channel.

## Settings

| Setting | Default | Meaning |
| --- | --- | --- |
| `ocomment.enable` | `true` | Run the server at all. |
| `ocomment.path` | `""` | The `ocomment` executable. Empty means the first one on `PATH`; a relative path is resolved against the workspace, and a leading `~` is expanded. |
| `ocomment.extraArgs` | `[]` | Extra arguments after `lsp`, such as `["--config", "tools/ocomment.toml"]`. |
| `ocomment.languages` | the 34 identifiers above | Which language identifiers the server is attached to. |
| `ocomment.trace.server` | `"off"` | Log the traffic to the output channel. |

Everything else — which comments count as removable, which are protected, per
directory overrides — is decided by `.ocomment.toml`, not by these settings.
See [configuration](https://github.com/P4suta/OComment/blob/main/docs/configuration.md).

## Removing comments on save

Either ask VS Code for the fix-all action:

```jsonc
{
  "editor.codeActionsOnSave": {
    "source.fixAll.ocomment": "explicit"
  }
}
```

or let the server do it, for every editor at once, from `.ocomment.toml`:

```toml
version = 1

[lsp]
on_save = true
```

The first is per-editor and per-language and can be scoped with
`[rust]`-style language sections; the second travels with the repository, so
everyone working in it gets the same behaviour.

## Trust

The extension launches the executable `ocomment.path` names, so it is disabled
in untrusted workspaces. Trust the folder, or leave `ocomment.path` unset and
install the binary yourself, before the server will start.

## Licence

MIT. The `ocomment` binary itself is MIT OR Apache-2.0.
