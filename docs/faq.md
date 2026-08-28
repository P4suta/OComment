# FAQ

## Does the default policy really remove documentation comments?

Yes. `safe` removes `doc-line` and `doc-block` along with ordinary comments; it
is *safe* in the sense that it never removes something another program reads,
not in the sense that it never removes something a human wrote. Documentation
comments are commentary, and a build artifact usually wants them gone.

If your project wants them kept — this one does — say so once:

```toml
[policy]
keep_kind = ["doc-line", "doc-block"]
```

[Why was this comment kept?](why-kept.md) has the full table of what each policy
does to each kind.

## Does a bare `ocomment` check the whole repository or just here?

Just here. A command that names no path checks the current directory, so running
it from a subdirectory checks that subdirectory. From the root of a repository
that is the whole repository:

```sh
cd "$(git rev-parse --show-toplevel)" && ocomment
```

Glob settings do not move with you. `files.include`, `files.exclude`, and every
`[[overrides]].paths` pattern is relative to the project root — the directory
holding `.ocomment.toml`, or the repository above it — however deep you run
from.

## Why did naming a path change what got scanned?

Because naming it is a request rather than a default. A walk with no path skips
hidden files and files over 32 MiB; an explicit `ocomment .` or `ocomment src`
bypasses those two limits, on the grounds that you asked for that path by name.
The binary and symlink safety checks still apply either way.

## Does it handle a partially staged file?

Yes, with `--staged`. `ocomment check --staged` reads the Git index blobs — the
exact bytes the commit will carry — instead of the working tree, so a file whose
comment is staged but whose other edits are not is judged on the staged half
alone. `ocomment fix --staged` rewrites the index blobs and maps those edits back
to the working tree only where the mapping is unique; `--index-only` is the
escape hatch when it is not.

The generated Lefthook hook deliberately does not use Lefthook's `stage_fixed`,
because that setting stages the whole working-tree file and destroys the partial
staging it was meant to protect.

## What happens to a file that is not valid UTF-8?

It is scanned anyway. The engine works on bytes, so a file with a Latin-1 name in
a string literal, or an encoding it has never heard of outside the regions it
edits, is read, reported on, and rewritten with those bytes untouched. Only the
spans it actually removes are changed.

A file that fails to *lex* — an unterminated block comment, say — is reported as
invalid and left alone, unless `--force-invalid` tells the run to apply the edits
that are still provably safe.

## Why is this comment still here after `fix`?

Ask it:

```sh
ocomment check --explain
```

`--explain` puts the rule that decided each comment, and the setting behind that
rule, on the line under it. Nine times in ten the answer is one of: the policy
does not remove that kind, a `keep_regex` or `keep_kind` in `.ocomment.toml`
protects it, an `[[overrides]]` entry matched the path, or the comment is a
directive that some other tool reads. See
[Why was this comment kept?](why-kept.md).

## What do the exit codes mean?

`0` clean, `1` findings, `2` failure. Specifically: `0` when nothing removable
was found and every requested change was applied, `1` when removable comments
were reported or a diff was printed, and `2` for an invalid source,
configuration, plugin, or I/O failure.

`1` from `diff` or `fix --dry-run` means the patch is not empty, which is why a
CI gate can be `ocomment check` with nothing around it, and why a script that
tests `$? -ne 0` will misread a non-empty diff as an error.

## Are CRLF line endings, BOMs, and the final newline preserved?

Yes, all three. A removal replaces the comment's own bytes and nothing else, so
a CRLF file stays CRLF, a UTF-8 BOM stays where it was, and a file with no
trailing newline does not grow one. Every layout also preserves the *line count*
of the file: a comment that spanned three lines is replaced by something that
still spans three lines, so line numbers in stack traces and `git blame` keep
pointing at the same statements. [Policies and layouts](policies.md) shows what
each one leaves behind.

## How fast is it, and how would I know?

Fast enough that it is not the slow part of a hook. The release gate refuses to
publish a build that misses any of: a 20 ms median cold `--version`, 500 MiB/s
for the simple scanners, 200 MiB/s for JavaScript and Shell, a 25 MiB stripped
binary, and no more than a 5% regression against the checked-in baseline on a
fixed machine. Where `typos` is installed, it also requires a no-op repository
scan to be no slower than 1.5 times `typos` on the same tree.

Those are gates rather than marketing numbers: measure on your own tree with
`ocomment -v`, which reports what was scanned and skipped.

## How much do I have to trust a scanner plugin?

Less than you would have to trust a normal plugin, by construction. A plugin is
a WebAssembly component that receives source bytes and returns comment spans and
kinds; it cannot edit a file, and the host rechecks the API version, the bounds,
the ordering, the overlap, the policy, and every edit generated from what it
returned. There is no WASI, no filesystem, no network, no clock, and no
randomness, and each invocation gets a fuel budget and explicit memory and
instance limits.

Remote artifacts need a pinned SHA-256 and a Sigstore identity, recorded in
`.ocomment.lock`, and are fetched only by an explicit `plugin add` or
`plugin update`. Ordinary scans and LSP sessions never go to the network. See
[Plugins](plugins.md).

## Does it work on Windows?

Yes. Every release publishes an `x86_64-pc-windows-msvc` archive with
PowerShell completions in it, plus the Scoop and WinGet manifests generated from
that archive, and CI smoke tests the binary on a Windows runner alongside Linux
and macOS. CRLF line endings are preserved rather than normalised, which matters
more on Windows than anywhere else. The container image is Linux-only, as
container images are.
