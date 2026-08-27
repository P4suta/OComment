# Editor and LSP setup

`ocomment lsp` is an LSP 3.18 stdio server. It negotiates UTF-8, UTF-16, or
UTF-32 positions, accepts incremental document changes, supports pull
diagnostics with a push fallback, and exposes comment, selection, document, and
workspace fixes. Diagnostics are hints by default. Save-time edits are disabled
unless `[lsp].on_save = true`.

OComment fixes are code actions, not formatting operations. Enable
`source.fixAll.ocomment` where an editor supports fix-all actions.
Workspace diagnostics and fixes report LSP work-done progress when the client
provides a token and advertise it as cancellable; `$/cancelRequest` aborts the
pending request with the standard cancellation response.

## Neovim

```lua
vim.lsp.config.ocomment = {
  cmd = { "ocomment", "lsp" },
  filetypes = {
    "rust", "ocaml", "c", "cpp", "go", "java", "javascript",
    "typescript", "python", "sh", "html", "css", "jsonc", "sql", "kotlin",
    "toml", "lua", "yaml", "php", "ruby", "zig", "r", "dart", "swift", "cs",
    "scala", "vue", "svelte", "markdown",
  },
  root_markers = { ".ocomment.toml", ".git" },
}
vim.lsp.enable("ocomment")
```

## Helix

```toml
[language-server.ocomment]
command = "ocomment"
args = ["lsp"]

[[language]]
name = "rust"
language-servers = ["rust-analyzer", "ocomment"]
```

Add `ocomment` to the `language-servers` list for each desired language.

## Zed

```json
{
  "lsp": {
    "ocomment": { "binary": { "path": "ocomment", "arguments": ["lsp"] } }
  }
}
```

Associate the server with the desired languages in the project or extension
configuration used by your Zed version.

## Emacs (Eglot)

```elisp
(add-to-list 'eglot-server-programs
             '((rust-mode c-mode c++-mode python-mode js-mode typescript-mode)
               . ("ocomment" "lsp")))
```

## VS Code

Install **OComment** from the Marketplace, or from Open VSX. The extension is
a client only: it launches the `ocomment` binary, which has to be installed
separately and on `PATH`, or named by `ocomment.path`.

It attaches to thirty-four language identifiers — `rust`, `ocaml`, `c`,
`cpp`, `objective-c`, `objective-cpp`, `cuda-cpp`, `go`, `java`, `javascript`,
`javascriptreact`, `typescript`, `typescriptreact`, `python`, `shellscript`,
`html`, `css`, `jsonc`, `sql`, `kotlin`, `toml`, `lua`, `yaml`, `php`,
`ruby`, `zig`, `r`, `dart`, `swift`, `csharp`, `scala`, `vue`, `svelte`,
and `markdown` — and contributes
`OComment: Remove comments in file`, `... in workspace`, `OComment: Restart
server`, and `OComment: Show output`, plus a status bar count of the removable
comments in the open files.

```jsonc
{
  "ocomment.path": "",
  "ocomment.extraArgs": [],
  "editor.codeActionsOnSave": { "source.fixAll.ocomment": "explicit" }
}
```

`[lsp].on_save = true` in `.ocomment.toml` does the same thing for everyone
working in the repository, rather than for one editor. The source is under
[`editors/vscode`](https://github.com/P4suta/OComment/blob/main/editors/vscode/README.md).

The extension is disabled in untrusted workspaces, because `ocomment.path`
names an executable it launches. Run `ocomment doctor` in the same environment
when the editor cannot start the process.

Any other LSP client can launch `ocomment lsp` directly; configure the document
selectors for the languages listed by `ocomment languages`. The server speaks
stdio and defines no transport flag, so a client that appends `--stdio` has to
be told not to.
