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
mode = "conservative"         # NOTE: conservative, standard, all
layout = "lines"              # NOTE: lines, columns, compact
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

Setting `files.follow_symlinks = true` permits read-only `check`, `scan`,
`diff`, and `fix --dry-run` operations to follow links. A real `fix`, including
an interactive one, refuses the whole transaction with exit code 2 if any
selected path is a symbolic link; neither the link nor its target is changed.

A command that names no path walks the current directory under those normal
limits; naming a path explicitly (`ocomment .`, `ocomment src`) is a request
rather than a default, so it bypasses the hidden-file and size limits.

`files.include`, `files.exclude`, and every `[[overrides]].paths` glob is
relative to the project root — the directory holding `.ocomment.toml`, or the
repository above it — however deep in the tree the command is run from.

Passing `--config FILE` replaces normal XDG and project discovery: only the
built-in defaults and that file are loaded. Its parent directory becomes the
root for globs and the plugin lock, while path arguments written on the command
line remain relative to the directory in which OComment was invoked.

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

YAML has one exception, and it is the only one in any language: a block scalar
decides where its body ends from the lines *below* it, so a whole-line comment
under a body is what terminates it and anything a removal writes on that line is
read back as part of the value. There every layout takes the whole line —
`lines` gives up that line's number and `columns` its columns rather than give
up the value. [Languages](languages.md#anything-else) states the rule in full.

[Policies and layouts](policies.md) shows all three on one sample.

## What a comment has to be, beyond its kind

A policy decides by kind, and a kind is a coarse thing to decide by. A one-line
`// NOTE:` explaining a decision and a forty-line essay above a function are
both `line`, and a project that wants the first and not the second cannot say
so with a policy. `[policy.allow]` is the other axes.

```toml
[policy.allow]
tags = ["NOTE", "SAFETY", "INVARIANT"]
max_lines = 1
trailing = false
```

- **`tags`** keeps a comment the policy would have removed, when its text opens
  with one of these. Matched against the comment's *text* — delimiters removed,
  and the `*` a block comment's continuation lines carry removed with them — so
  one rule holds in every language. This is what `keep_regex` cannot do: a
  pattern is matched against the whole raw token, so `^//\s*NOTE` protects a
  Rust comment and silently fails to protect the identical rule written in Lua,
  where the token opens `--`.

  A tag is a word rather than a prefix: `NOTE` allows a note and does not allow
  `NOTEBOOK`. What may follow it is punctuation or space — `NOTE:`,
  `TODO(alice)`, `FIXME -` — or nothing at all.
- **`max_lines`** removes a comment, or a run of comments on consecutive lines,
  that occupies more lines than this. A run is measured rather than a single
  token because four consecutive `//` lines are four comments to a scanner and
  one paragraph to a reader, and the reader is right.

  A blank line ends a run: that is how a writer says the next remark is a
  separate remark. A limit that counted across one would be measuring the gap
  as well as the prose.
- **`trailing = false`** removes a comment sitting after code on the same line.
  It closes the obvious way around a rule about comments above code, which is
  to put the comment beside it instead.

### What these rules reach, and what they do not

They cut across the policy rather than under it: a comment failing one is
removed whatever the *policy* said about its kind, a tag included. A tagged
comment still has to be short enough and still may not sit beside code.

Two things are out of their reach, and both for the same reason — the rules are
about commentary, and neither of these is commentary.

**A comment somebody named outright.** `keep_kind` names a kind and
`keep_regex` names the bytes; both are a project saying *keep exactly this*,
and a shape rule is a project saying *keep things like this*. The specific
wins. This repository pins every GitHub Action to a SHA and writes the version
beside it as a comment that Dependabot rewrites — a `keep_regex` names it, and
it has to sit beside the line it annotates.

**A comment that is not prose.** Documentation comments and licence notices are
as long as their content requires. A directive is *addressed* to a tool, and a
tool reads it where it sits: `x = 1  # noqa` silences a warning about that line
and silences nothing a line above it. The protections no policy reaches — a
shebang, an encoding line, a directive the language or its build reads — are
out for the reason they are always out.

## How a comment that survives is written

The rules above decide what stays.
`[style]` decides how what stays reads, and it is a table of its own for that reason: a comment that fails one of the rules above is *removed*, and a comment that fails one of these is *rewritten*.
One table whose entries have two different consequences is a table nobody can add to safely.

```toml
[style]
space_after_marker = true
trailing_whitespace = false
```

Every rule here is off unless you turn it on.
A formatter that starts reformatting a repository because it was installed is a formatter somebody uninstalls.

- **`space_after_marker = true`** rewrites `//text` as `// text`.
  It says nothing about a comment that already has a space, and nothing about a marker with no text after it: a bare `//` is a blank line in a paragraph rather than a comment missing its space.
  It is deliberately timid about what counts as text — it acts only when the first character is neither white space nor ASCII punctuation — so a ruler like `////////` or `#####` or `//------` comes back unchanged.
- **`trailing_whitespace = false`** strips white space from the end of every line a comment covers, the last one included.
  A line comment's span ends where its text ends, so the spaces `// note   ` trails are inside it.
  What a *removal* leaves behind is the layout's business and is not touched here.

An `[[overrides]]` entry may carry its own `[style]`, which replaces the global one whole rather than merging into it, for the reason `[policy.allow]` does.

### What the style rules reach

Almost the mirror of the rules above, and the one place they disagree is the point.

| Kind | `[policy.allow]` | `[style]` |
| --- | --- | --- |
| `line`, `block`, `html-comment` | yes | yes |
| `doc-line`, `doc-block` | no | **yes** |
| `license` | no | no |
| `directive`, `shebang`, `encoding`, `load-bearing`, `optimizer-hint`, `version-comment` | no | no |

A documentation comment is exempt from the length rule *because* it is documentation — it is as long as its content requires.
That same fact is why it is the first thing the style rules should reach: it is the prose in a repository that most readers actually read, and it is the prose nobody has a tool for.

A licence notice is out, and out more firmly than anything else.
It is a legal text quoted verbatim, and verbatim is the whole of its value; a formatter that tidied one would be changing a document the project does not own.
The directives and the preamble are out for the reason they are always out: a tool reads them, a tool is not a reader, and rewriting bytes something parses is how a tidy-up changes what a build does.

A comment whose bytes are not valid UTF-8 is never rewritten.
The engine does not decode a whole source, and a boundary guessed at inside bytes it could not read is how a formatter corrupts a file it was asked to tidy.

### Removing nothing

`mode = "none"` is the policy for a repository that wants the style rules and not the removals.

```toml
[policy]
mode = "none"

[style]
space_after_marker = true
trailing_whitespace = false
```

It sits at the weak end of the scale the other three already form, and it answers before the kind table rather than inside it, so every kind is kept for the same reason.
Saying this used to mean listing every comment kind under `keep_kind`, which is a setting that has to be revisited each time a kind is added: it said "these twelve kinds" when what it meant was "all of them".

It is the policy default and not the first word.
`remove_kind` and `remove_regex` still name comments outright, and a comment they name still goes.

### Taking stock of the convention

```console
$ ocomment tags
  INVARIANT	101
  NOTE	1482
  PERF	7
! XXX	4

Allowed and never written: SAFETY.

Written and not allowed: XXX (4). These are comments this run removes today;
add one to `[policy.allow] tags` to keep it, or to `[policy.allow.expiry]` to
keep it for a while.
```

A convention drifts in two directions and the report looks both ways. A tag
nobody writes any more is a line of configuration that protects nothing and
reads like a rule; a tag people write that nobody configured is a comment the
run removes today, which is usually the first anybody hears of it. The `!` is
the second kind.

It reports rather than gates, as `ocomment coverage` does: what to do about an
unconfigured tag is a decision about that tag, and a run that failed would be
making it for you. `--format json` gives the same four lists as fields.

The question it asks is not quite the one `tags` matches. The rule asks *does
this comment carry the tag `NOTE`?*, and `// NOTEBOOK entry` does not; the
inventory asks *what word does this comment open with?*, and the answer there
is `NOTEBOOK` — which is what you want to see, because that comment is one the
run removes and the listing is where you would find out.

### Tags that are promises

A `TODO` is not the same kind of thing as a `SAFETY`. One records why the code
is the way it is and is true for as long as the code is; the other says
somebody will do something, and saying so is not doing it.

A rule that treats them alike has to pick a bad answer. Forbid the `TODO`, and
nobody obeys it — the note is lost along with the nagging. Permit it, and the
repository ends up carrying one from four years ago that everybody has learned
to read past.

```toml
[policy.allow.expiry]
TODO = "30d"
FIXME = "14d"
```

A tag here is allowed exactly as one in `tags` is, until the line carrying it
reaches that age — and is a finding after that, every run, until somebody
either does it or deletes it:

```console
$ ocomment check --explain src/pool.rs
src/pool.rs:44:1: removable line comment: // TODO: retry on timeout
    removed: `TODO` is a promise with 30d to keep it, and this line is 61d old ([policy.allow] in .ocomment.toml); do it, or delete the comment
...
1 comment past its deadline: 1 TODO. Do it or delete it.
```

The age is the age of the commit that introduced the line, read from
`git blame`. Writing one therefore costs nothing: a `TODO` you typed a minute
ago belongs to no commit and has not started counting, and `"0d"` means the
deadline starts at the next commit. Ages are written `"14d"`, `"2w"`, or as a
bare number of days; an hour is not a meaningful deadline for a line of source
and a month is not a fixed number of days, so neither is accepted.

Three consequences worth knowing:

- **`ocomment fix` deletes an overdue promise**, because the rule is the same
  rule and `fix` applies the rules. That is the half of "do it or delete it" a
  machine can do.
- **`ocomment check --staged` never reports one.** A staged run judges the
  lines the commit adds, and a line the commit adds is new. The pre-commit gate
  is about what you are writing; the deadline is about what the repository has
  been carrying.
- **No repository, no clock.** Outside a Git repository, or on a file Git does
  not track, the age cannot be read and the comment is left alone. A deadline
  nobody can measure has not passed.

## Getting to a rule you cannot turn on today

A repository with eleven thousand comments and a rule it wants to reach has two
bad options: turn the rule on and fail every commit, or leave it off and never
arrive. A ledger is the third.

```toml
[ratchet]
ledger = ".ocomment-ledger"
```

`ocomment ratchet --update` records what each file holds today.
`ocomment ratchet` checks the tree against that record and fails when a file
holds **more** — and fails when it holds **fewer**, asking for the ledger to be
updated.

That second direction is what makes it different from a baseline file. A
baseline forgives what it recorded and says nothing once the work is done; a
ledger that only noticed growth would eventually describe a repository that no
longer exists. Checked both ways, the number in the file is always the number
in the tree, and the distance left to go is readable at a glance:

```
# 1674 comment(s) in 78 file(s) left to remove.
3 .dockerignore
54 src/legacy/session.c
```

It is deliberately not a suppression mechanism. The entries carry no reasons,
no expiry dates and no per-comment granularity — a ledger is a measurement, and
the moment it starts explaining itself it has become a second configuration
file arguing with the first.

This repository does **not** run one, and that is deliberate. It reached its
own rules by fixing every comment that broke them rather than by recording how
many did: `ocomment` over this tree exits 0, and the CI job that runs it is a
gate rather than a report. A ledger is for a repository that cannot get there
today; a tool's own repository does not get to be that repository.

## Files another tool writes

A lock file, a recorded seed list, a code generator's output: something else
wrote the comments in these and will write them again. Removing one is editing
a tool's file, and it is the class most likely to be auto-fixed without being
read, because nobody opens a generated file before committing it. Coverage of a
file you should not touch is worse than skipping it — a skip is visible in the
summary, and a removal in a generated file is a diff somebody waves through —
so `--deny-skipped` does not refuse these, and `--include-generated` scans them
anyway for the run that means it.

`spec/generated.toml` is the catalogue. It lists whole file names, because a
lock file's name is a convention of the tool that writes it; suffixes, matched
without the dot and case-insensitively; and the headers a generated file
announces itself with, searched case-insensitively in the first `header_lines`
lines only.

That bound is what makes the header search usable at all. A file that *lists*
these markers would otherwise claim itself, and two in this repository do —
`spec/directives.toml` names C#'s `<auto-generated`, and the catalogue names
all of them. A generated file declares itself in its first few lines, because
that is where the reader it is warning will look, so the bound costs nothing
real and rules out every catalogue, changelog and piece of documentation that
merely mentions one.

A lock file, a recorded seed list, a code generator's output: the comments in
these belong to the tool that wrote them and come back on its next run.
Removing one is editing a tool's file, and it is the class most likely to be
auto-fixed without being read, because nobody opens a generated file before
committing it.

OComment passes them over, under a skip reason of their own that
`--deny-skipped` does not refuse — being passed over is what should happen to
them. A file is recognised by name (`Cargo.lock`, `package-lock.json`,
`*.proptest-regressions`, and the rest of `spec/generated.toml`) or by the
header a format uses to say so: `@generated`, `DO NOT EDIT`, `Code generated
by` and their neighbours, in the first five lines only.

The line bound matters. Without it a file that *lists* those markers claims
itself — this repository's own catalogue of protected directives names C#'s
`<auto-generated`, and did exactly that. A generated file declares itself at
the top, where the reader it is warning will look.

```toml
[files]
include_generated = true      # NOTE: scan them anyway
```

`--include-generated` does the same for one run.

## Declarative language profiles

Profiles cover delimiter-based syntaxes. Empty definitions are rejected while
loading configuration, and so are two delimiters spelled the same way — nothing
could choose between them.

One comment token being the *start* of another is not ambiguous and is not
refused. It is how a language spells a documentation comment: Gleam writes
`//`, `///` and `////`, Haskell writes `--` and `-- |`, WIT writes `//` and
`///`. The scan takes the longest token that matches, so the order the
delimiters are written in carries no meaning and an author cannot get it
wrong.

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

[[profiles.lisp.protected_patterns]]
contains = "lisp-build:"
reason = "read by the build"
tier = "load-bearing"
```

A protected pattern says how strongly it asks. `tier = "tool"` is the default
and the weaker one: the comment is recorded as a `directive`, every policy but
`all` keeps it, and `all` is entitled to take it. `tier = "load-bearing"`
records it as `load-bearing`, which no policy removes and only
`--force-protected` does.

The distinction is the one the built-in languages already draw, and a profile
needs it for the same reason. A profile describes a syntax OComment has no
scanner for, so its author is the only one who knows whether a marker is read
by their toolchain or by their linter — and removing the first changes what the
build produces while removing the second changes what a tool reports. Declaring
the stronger tier is deliberate: a pattern that says nothing gets the weaker
one, which is what every profile written before this field meant.

Complex lexical grammars should use a WASM scanner plugin instead. A plugin
returns the comment kind itself, so it can return `load-bearing` directly.

### The profiles OComment ships with

`spec/profiles.toml` carries a few, and `ocomment profiles` lists them — with
the ones this project declared or replaced marked as its own. They have a
listing of their own rather than a place in `ocomment languages` because they
are not built-in languages and are not meant to become one: a built-in language
is a hand-written scanner, and a format earns that when its lexical form has
something a delimiter list cannot say — a string
that hides a comment token, a nesting rule, an embedded language. The formats
here have none of that, so a profile says everything there is to say, and says
it in data rather than in a `match` arm that would then have to be written
twice, once in Rust and once in OCaml.

They exist because of what `ocomment coverage` reported without them. A gate
that says "no removable comments in 143 files" while 25 files were never opened
is a gate over 85% of a repository, and the files it was missing here were
`.gitignore`, `CODEOWNERS`, `dune` and OComment's own `.wit` interface — every
one of which holds comments.

**A profile added to a build changes what a gate covers.** Files the previous
version passed over in silence are read by the next one, and the comments in
them are reported against the same policy as everything else — the reader does
not change the verdict, and a `#` line of prose is prose wherever it sits. That
is deliberate: a rule that softened for files a profile happened to read would
mean the same bytes getting different answers from different readers. What must
not be silent is the change in what is *read*, so `ocomment coverage` names the
reader for every file it scanned, and the count that moves out of `no built-in
language for this file` and into `read by the \`hash-line\` profile` is the size
of the change. A project that wants the prose in those files kept says so where
every other such decision is said:

```toml
[[overrides]]
paths = [".gitignore", ".gitattributes", ".github/CODEOWNERS"]
keep_kind = ["line"]
```

- **`hash-line`** is the pattern-list family: `CODEOWNERS`, the `ignore` files,
  `.editorconfig`, and OComment's own ledger. A `#` opens a comment here only
  as the first byte of its line. Anywhere else it belongs to the pattern —
  `file#name` names a file with one in it, `\#literal` is how a pattern that
  starts with one is written, and a `#` after the pattern is still the pattern.
  Reading any of those as a comment wrote a shorter pattern back, so a default
  `fix` quietly stopped ignoring what the line named.
- **`hash-anywhere`** is the other half: `.gitmodules` and `.opam`, whose
  syntaxes do let a comment open after a value. It is separate rather than
  sharing the rule above because the two rules disagree about the same byte,
  and a profile that has to be right about both is right about neither.
- **`dune`** is a Lisp: `;` opens a line comment, `"..."` is a string with
  backslash escapes so a `;` inside one is text, and `#|...|#` nests.
- **`go-module`** is `go.mod` and `go.work`: `//` to end of line, no block
  comment, no string form a `//` could hide inside. Two markers in them are
  kept, at the strength each has earned. `// indirect` is addressed to
  `go mod tidy`, which puts it back, so it is a directive — every policy but
  `all` keeps it. `// Deprecated:` is put back by nothing: before the module
  declaration it is what `go get` warns with and what a proxy serves to
  everyone downstream, and inside a `retract` block it is the reason
  `go list -m -retracted` prints, so no policy reaches it. A profile matches a
  substring, so each also claims a comment that merely opens with the same
  words — which is the tier's other job, because a line of prose caught by
  `// indirect` is kept by a gate rather than put beyond every policy there is.
  `go.sum` and `go.work.sum` are not here: they are lock files, and
  `spec/generated.toml` is where a file another tool writes belongs.
- **`wit`** is the Component Model's interface language, which OComment's own
  plugin contract is written in. `///` documents the item below it and `//` is
  a remark, and the two are listed side by side. They could not be, once: a
  delimiter that was the start of another was refused as ambiguous, so WIT's
  documentation comments were reported as ordinary line comments and a default
  policy was entitled to remove them.
- **`gleam`** has three line comment forms and nothing else: `//` is a remark,
  `///` documents the item below it, and `////` documents the module. All three
  share a prefix, which is exactly what the longest-token rule exists for.
- **`haskell`** is the one that needed new vocabulary. Its comment opener is a
  *run* of dashes, and whether it opens a comment depends on what follows the
  run: `-- x` is a comment, `-->` and `---->` are operators, and `---x` is a
  comment again. `forbidden_after` states that clause (Haskell 2010 §2.2).
  Haddock marks only the first line of a documentation comment and continues it
  with the plain opener, so the profile sets `doc_continuation`; without it the
  conservative policy would keep the first line of a published page and remove
  the rest. `{-|` and `{-` both close with `-}` and both nest, and the nesting
  count is kept against the closing token rather than the opener — a remark
  nested inside documentation still has to be got past. Literate Haskell is
  deliberately absent: a `.lhs` file is a different format, where code is what
  is marked up rather than prose.

A `[profiles.<name>]` entry in your configuration wins over the shipped profile
of the same name, so a project that disagrees with one can replace it rather
than work around it.
