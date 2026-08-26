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

A command that names no path walks the current directory under those normal
limits; naming a path explicitly (`ocomment .`, `ocomment src`) is a request
rather than a default, so it bypasses the hidden-file and size limits.

`files.include`, `files.exclude`, and every `[[overrides]].paths` glob is
relative to the project root — the directory holding `.ocomment.toml`, or the
repository above it — however deep in the tree the command is run from.

## Layouts

`layout` decides what a removal leaves behind. It moves bytes, never decisions:
no comment is kept or removed because of it.

| Layout | What a removal leaves in place of the comment |
| --- | --- |
| `lines` | The default. The line terminators the comment spanned, so every following line keeps its number, and a single space where the comment was all that kept two tokens apart. |
| `columns` | As `lines`, plus spaces of the comment's own display width, so every following column keeps its number as well. A tab counts to the next multiple of eight. |
| `compact` | As `lines`, except that a line which held nothing but a removed comment goes away with it, terminator included, and the whitespace a removal would leave at the end of a line is trimmed. |

`compact` never touches a line that code survives on. Such a line keeps its
terminator and its CRLF or LF style, and a comment running across several lines
with code before or after it closes up to a single line rather than joining two
statements. A surviving line keeps the ending it had in the source — the same
LF or CRLF, from inside the comment if that is where it was — or no ending at
all if the file stopped there without one. Being alone on a line is judged from
the original bytes, so a line holding two comments and nothing else keeps its
terminator: neither of them was alone on it.

[Policies and layouts](policies.md) shows all three on one sample.

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
