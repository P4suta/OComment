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

OComment intentionally has no editor-specific extension. A generic LSP client
can launch `ocomment lsp`; configure the document selectors for the languages
listed by `ocomment languages`. Run `ocomment doctor` in the same environment
when the editor cannot start the process.
