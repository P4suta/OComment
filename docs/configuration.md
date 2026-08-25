# Configuration

OComment reads `.ocomment.toml` version 1. Values are merged in this order:
built-in defaults, the XDG user file, the nearest project file, matching path
overrides, and command-line flags. Unknown keys and incompatible dialects are
errors. `ocomment config locate`, `show`, `explain`, and `schema` expose the
resolved state.

The user file is `$XDG_CONFIG_HOME/ocomment/config.toml`, or the platform's
standard config directory when `XDG_CONFIG_HOME` is unset.

```toml
version = 1

[files]
max_size = 33554432
hidden = false
follow_symlinks = false
ignore = true
include = []
exclude = ["vendor/**"]

[policy]
mode = "safe"                 # safe, legal, all
layout = "lines"              # lines, columns, compact
keep_kind = ["directive"]
remove_kind = []
keep_regex = ["(?i)generated"]
remove_regex = []
force_invalid = false
force_protected = false

[git]
staged = false
index_only = false

[lsp]
on_save = false
diagnostics = true
code_lens = true

[languages.sql]
dialect = "postgresql"

[[overrides]]
paths = ["fixtures/**"]
policy = "all"
layout = "compact"
```

Normal repository walks honor `.gitignore`, `.ignore`, and `.ocommentignore`,
skip hidden files, binary files, symlinks, and files larger than 32 MiB. An
explicit file or directory bypasses the hidden and size limits. Binary and
symlink safety checks still apply.

## Declarative language profiles

Profiles cover unambiguous delimiter-based syntaxes. Ambiguous or empty
definitions are rejected while loading configuration.

```toml
[profiles.lisp]
extensions = ["lisp", "cl"]

[[profiles.lisp.line_comments]]
start = ";"
kind = "line"

[[profiles.lisp.block_comments]]
start = "#|"
end = "|#"
nested = true
kind = "block"

[[profiles.lisp.strings]]
start = "\""
end = "\""
escape = "\\"

[[profiles.lisp.protected_patterns]]
contains = "ocomment: keep"
reason = "local directive"
```

Complex lexical grammars should use a WASM scanner plugin instead.
