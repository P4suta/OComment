# Contributing to OComment

Thank you for helping improve OComment. Bug reports, language fixtures,
documentation, performance data, and code changes are all welcome.

## Before opening a change

- Use [GitHub Discussions](https://github.com/P4suta/OComment/discussions) for
  design questions and support.
- Use an issue for confirmed bugs and scoped feature requests.
- Report vulnerabilities through the private process in [SECURITY.md](SECURITY.md).

Substantial scanner, policy, public API, Git, LSP, or plugin-contract changes
should have an agreed design before implementation. Small fixes can go directly
to a pull request.

## Development setup

The production workspace requires Rust 1.88 or newer. Differential verification
also requires OCaml 5.5, opam, Dune 3.24.2, Python 3, and the dependencies from
`ocaml/ocomment-ref.opam`.

```sh
opam install ./ocaml/ocomment-ref.opam --deps-only --with-test
cargo build --manifest-path rust/Cargo.toml --workspace --locked
```

The repository is intentionally split into independent implementations:

- `spec/` contains shared contracts and fixtures.
- `rust/` contains the product, public library, LSP server, and plugin host.
- `ocaml/` contains the independent reference implementation.

Do not share scanner code between Rust and OCaml. Matching normalized outputs
are the cross-check.

## Required checks

Run the checks relevant to your change; scanner or policy changes should run all
of them.

```sh
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo test --manifest-path rust/Cargo.toml --workspace --all-targets --locked
opam exec -- dune runtest --root ocaml
opam exec -- ./tools/differential.sh
python3 tools/check_embedded_specs.py
python3 tools/validate_schemas.py
./tools/package-list.sh
actionlint
```

When behavior changes, add the smallest fixture that proves the lexical edge
case. Keep byte spans half-open, edits sorted and non-overlapping, and output
deterministic. Update both implementations and their differential expectations
when the shared contract changes.

## Pull requests

- Keep each pull request focused and explain compatibility or safety effects.
- Add tests for observable behavior and update user-facing documentation.
- Regenerate checked-in schemas, WIT, man pages, or completions when their source
  changes; `tools/check_embedded_specs.py` checks shared embedded assets.
- Do not include build output, credentials, or unrelated formatting changes.

The repository uses squash merges. By submitting a contribution, you agree that
it is licensed under either MIT or Apache-2.0, at the user's option.
