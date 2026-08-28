# Getting started

## Install it

```sh
cargo install ocomment --locked
```

That is one of several channels — a prebuilt archive, Homebrew, Scoop, WinGet,
a container image, a GitHub Action, and a pre-commit hook are all documented
under [Installation](installation.md). Everything below works the same way
whichever one you used.

## Look before you leap

`ocomment` with no command is `ocomment check`, and `check` with no path is the
current directory, so the shortest useful run is the tool's own name:

```console
$ ocomment check src
src/main.rs:2:5: removable line comment: // TODO: drop this
Found 1 removable comment in 1 file (1 file scanned). Run `ocomment fix` to remove it.
```

Nothing has changed on disk. `check` only reports, and it reports the same set
of comments that `fix` would remove.

## See the change as a patch

```console
$ ocomment diff src
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,4 +1,4 @@
 fn main() {
-    // TODO: drop this
+    
     println!("hello");
 }
Found 1 removable comment in 1 file (1 file scanned). Run `ocomment fix` to apply the patch.
```

The patch goes to standard output and the summary line goes to standard error,
so `ocomment diff src > fix.patch` writes a file `git apply` will take. The
blank line the removal leaves behind is the `lines` layout at work; `compact`
and `columns` leave something else, and [Policies and layouts](policies.md)
shows all three side by side.

`ocomment fix --dry-run src` prints the same patch and applies nothing, which is
the form to reach for inside a script.

## Make the change

```console
$ ocomment fix src
fixed src/main.rs: removed 1 comment
Removed 1 comment in 1 file (1 file scanned).
```

Every edit of a run is prepared first and committed as one transaction, so an
interrupted `fix` leaves the tree as it found it rather than half-rewritten.

`ocomment fix -i` asks about each comment instead, with three lines of context
either side: `y` removes it, `n` keeps it, `a` and `d` answer for the rest of
the file, `q` stops asking and applies what was accepted, and `x` abandons the
run without writing anything.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Nothing removable was found, and every requested change was applied. |
| `1` | Removable comments were reported, or a diff was printed. |
| `2` | An invalid source, configuration, plugin, or I/O failure. |

That is why `ocomment check` works as a CI gate on its own, and why `1` from
`diff` is not an error: it means the patch is not empty.

## Decide what your project keeps

The default `safe` policy removes ordinary and documentation comments and keeps
source preambles and tool directives. Write the decision down instead of
passing flags every time:

```sh
ocomment init config
```

`init config` writes a `.ocomment.toml` holding every default spelled out, so
the file starts as a complete description of what the tool already does and you
change the lines you disagree with. A project that has made a few decisions
ends up looking like this:

```toml
version = 1

[policy]
mode = "legal"
layout = "lines"
keep_kind = ["doc-line", "doc-block"]
keep_regex = ['^//\s*NOTE\b']

[[overrides]]
paths = ["generated/**"]
policy = "all"
```

That file keeps licence headers, keeps documentation comments, keeps any
comment opening with `NOTE`, and takes everything out of `generated/`.
[Configuration](configuration.md) documents every key, and `ocomment config
explain` prints the resolved result with the source of each value.

## Ask why a comment survived

```sh
ocomment check --explain
```

`--explain` puts the rule that decided each comment, and the setting behind that
rule, on the line underneath it — for the comments it kept as much as the ones
it would remove. [Why was this comment kept?](why-kept.md) walks through a real
answer.

## Put it in the loop

```sh
ocomment init lefthook --fix
lefthook install
```

The generated hook runs `ocomment check --staged`, which judges the bytes the
commit will actually carry rather than the working tree — the distinction that
matters for a partially staged file. [CI and hooks](ci.md) covers the
pre-commit manifest, the composite GitHub Action, and SARIF upload to code
scanning; [Editors and LSP](editors.md) covers seeing the same diagnostics as
you type.
