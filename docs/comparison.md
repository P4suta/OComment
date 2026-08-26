# Comparison

Several well-known tools remove comments from source code, and most of them were
built for a different job than OComment was. This page is about scope, not
quality: each of these does its own job well, and the useful question is which
job you have.

> The rows below describe each project's **documented purpose and scope**, taken
> from its own documentation, and were not benchmarked or feature-tested here.
> Tools change; check the current documentation of anything on this page before
> relying on a row. Only the OComment column describes software this book can
> check: the rest is read off other projects' own documentation.

| | OComment | `strip-comments` | `decomment` | `cloc --strip-comments` | `gcc -E -fpreprocessed` |
| --- | --- | --- | --- | --- | --- |
| What it is | A comment checker and remover | A Node.js library and CLI for stripping comments | A Node.js library for stripping comments | A line counter, with a comment-stripping side output | A C preprocessor |
| Built to | Report, gate, and remove comments under a policy | Strip comments from JavaScript-style source | Strip comments while preserving string literals | Count lines of code | Preprocess C-family translation units |
| Language coverage | [19 languages and 16 dialects](languages.md), plus declarative profiles and WebAssembly plugins | JavaScript and other C-style syntaxes | JavaScript, JSON, CSS, HTML | Very broad, from its own per-language comment table | C, C++, Objective-C, and their preprocessed inputs |
| Keeps tool directives by default | Yes — shebangs, encoding preambles, `//go:build`, lint controls, optimiser hints, MySQL versioned comments | Documents an option for keeping `/*!` "protected" comments | Documents an option for keeping `/*!` "protected" comments | Not a stated goal | Not a stated goal |
| Configurable per path | Yes, `[[overrides]]` globs in `.ocomment.toml` | Through the calling program | Through the calling program | No | No |
| Check-only mode with a CI exit code | Yes, `ocomment check` | No | No | No | No |
| Machine-readable report | JSON, JSONL, SARIF, GitHub annotations | No | No | Counts, in several formats | No |
| Rewrites files in place | Yes, as one rollback-backed transaction | Through the calling program | Through the calling program | Writes a stripped copy of each file | Writes to standard output |
| Non-UTF-8 input | Scanned and preserved byte for byte | Not stated | Not stated | Not stated | Set by `-finput-charset` |
| Keeps line numbers | Yes under `lines` and `columns`, but for the one YAML line a block scalar would read back; `compact` gives them up by design | Not stated | Not stated | Not stated | Rewrites line structure, and emits line markers |
| Editor integration | LSP 3.18 server, VS Code extension | No | No | No | Not applicable |
| Independent cross-check | An OCaml reference implementation compared on every fixture | — | — | — | — |
| Installs as | A single static binary, or a crate | An npm package | An npm package | A Perl script or package | Part of a C toolchain |

## When something else is the right tool

- **You want a count, not a rewrite.** `cloc` answers "how much of this is
  comment?" directly, across more languages than any comment remover needs to
  support, and writing the stripped copies is a side output of that.
- **You are already inside a Node build step**, transforming strings in memory
  rather than files on disk. A library you can call is less friction than a
  binary you have to install, and `strip-comments` and `decomment` are libraries
  first.
- **You are preprocessing C anyway.** If the compiler is already running over
  the translation unit, `gcc -E` has removed the comments as part of the job.
  Note that it is doing much more than that — macro expansion, includes, line
  markers — so its output is not the same file minus comments.

## What OComment adds

- **A policy, not a switch.** A comment that another program reads is not
  commentary, and the default keeps every one it recognises. See
  [Why was this comment kept?](why-kept.md).
- **An answer to "why".** `--explain` names the rule and the setting behind
  every decision, which is what makes a house rule reviewable rather than
  mysterious.
- **A gate.** `ocomment check` exits `1` on findings and speaks SARIF, so the
  same tool that removes comments can hold a line in CI and in a pre-commit
  hook. This repository uses it on itself.
- **Bytes in, bytes out.** BOMs, CRLF, missing trailing newlines, and non-UTF-8
  bytes outside the edited spans survive a rewrite, and every removal is
  committed as one transaction.
- **A second implementation.** The OCaml reference implementation shares no code
  with the Rust one, and the two are compared on the scanner, the
  classification, the diagnostics, the edits, the transformed bytes, and the
  source maps.
