# OComment

OComment is a fast, byte-preserving comment checker and remover. It reads source
bytes without requiring them to be UTF-8, reports every comment it finds, and
removes the ones a policy allows it to remove — through a rollback-backed
transaction, so a run either applies every edit or none of them.

The production tool is the Rust `ocomment` binary and the public `ocomment-core`
library. `ocomment-ref` is an independent OCaml implementation, and the two are
compared on the scanner, the classification, the diagnostics, the edits, the
transformed bytes, and the source maps. Nothing is shared between them; matching
normalized output is the cross-check.

```console
$ ocomment check src
src/main.rs:2:5: removable line comment: // TODO: drop this
Found 1 removable comment in 1 file (1 file scanned). Run `ocomment fix` to remove it.
```

## What it is for

A comment remover is usually wanted for one of three reasons, and OComment is
built for all three:

- **Shipping less than you wrote.** Stripping comments out of a build artifact,
  a container image, or a vendored copy, without changing what the code does.
- **Holding a line in review.** A team that has agreed which comments are worth
  keeping can encode that agreement in `.ocomment.toml` and let the
  [pre-commit hook or CI](ci.md) enforce it. This repository does exactly that
  to itself.
- **Reading what a file really says.** `ocomment scan` gives the kind, the
  disposition, and the byte span of every comment, for a tool to consume.

## What it will not do

It will not remove a comment that is not commentary. A shebang, an encoding
preamble, a build tag, a lint control, an optimiser hint, and a MySQL versioned
comment all change what some other program does with the file, and the default
policy keeps every one of them. [Why was this comment kept?](why-kept.md) is the
page about that, and `--explain` answers it for any single comment.

It will not corrupt a file it does not understand. The engine never requires the
complete source to be UTF-8, so BOMs, CRLF line endings, trailing newlines, and
non-UTF-8 bytes outside the edited spans come back exactly as they went in.

## Where to go next

- [Getting started](getting-started.md) is the five-minute version.
- [Installation](installation.md) lists every channel the tool ships through.
- [Commands](commands.md) is the complete CLI reference, generated from the
  binary.
- [Configuration](configuration.md) is `.ocomment.toml` in full.
- [Policies and layouts](policies.md) shows what each setting does to one
  sample file.
- [Languages and dialects](languages.md) is what OComment can read.
- [Editors and LSP](editors.md), [CI and hooks](ci.md), and [Docker](docker.md)
  cover the integrations.
- [Library](library.md) and [Plugins](plugins.md) are for building on it.

OComment is available under either the
[MIT license](https://github.com/P4suta/OComment/blob/main/LICENSE-MIT) or the
[Apache License 2.0](https://github.com/P4suta/OComment/blob/main/LICENSE-APACHE),
at your option.
