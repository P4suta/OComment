# Scanner decision assurance

`keep_regex` protects every member of a contiguous comment block of the same kind when any member matches.
The [configuration contract](configuration.md) defines its boundaries and precedence.
`PreparedScanner::scan`, explanation output, and incremental scans use this decision before shape rules.
Style rules still apply to kept comments.

## Checked properties

The [proof inventory](https://github.com/P4suta/OComment/blob/main/proofs/inventory.toml) imports byte-for-byte snapshots of the actual Rust decision modules and proof admission function.
It checks these bounded properties with Kani 0.68.0, pinned in [the proof tool configuration](https://github.com/P4suta/OComment/blob/main/proofs/mise.toml).

| Property | Input domain |
| --- | --- |
| Adjacency means whitespace containing exactly one logical line ending | Every byte sequence of length 0 through 16, including LF, CRLF, and CR |
| A member inherits the earliest configured match from exactly its connected run | Every sequence of 0 through 8 members, arbitrary join flags, and optional machine-sized pattern indices |
| Proof admission requires all six evidence facts | Every combination of the six Boolean facts |

Each nonempty block harness requires reachable kept and removed outcomes for its selected member.
The empty block has one possible outcome and requires its own reachable empty witness.
The adjacency and admission harnesses each require reachable acceptance and refusal witnesses.
Every declared contract assertion must report success even when compiler-generated library assertions are proven unreachable.
Separate block harnesses cover each length from 0 through 8 while keeping member data and the selected index arbitrary.
Each nonempty case checks an arbitrary member, so it covers every index without repeating the specification for each index in one execution.
Unwind bounds are 17 for at most 16 gap bytes and the member count plus one for each block case, with unwinding assertions enabled.
Six separate rejecting probes assert specific false claims about boundaries, propagation, and admission.
The gate requires their named assertion failures rather than accepting an arbitrary panic, solver error, or unwinding failure.
These probes test the proof boundary; they are separate from the positive proofs.

## Biome suppression headers

The classifier recognizes the four headers in [Biome's suppression syntax](https://biomejs.dev/analyzer/suppressions/).
Each header must begin a comment-content line and end at whitespace or the end of that line.
A block comment can put decorative stars before the header.
A line comment cannot use those stars to hide a prose prefix.
The existing comment normalization folds case before this decision.
Biome validates suppression categories, arguments, and range placement.
OComment classifies the header without duplicating those validations.
The common classifier also handles comments from a declared GraphQL profile.
This change adds no built-in GraphQL scanner.

| Property | Input domain |
| --- | --- |
| Exactly four keywords have the specified suppression identity and name | Every byte sequence of length 0 through 24, including invalid UTF-8 and longer words sharing a marker prefix |

The keyword harness uses an unwind bound of 25 with unwinding assertions enabled.
It requires a reachable witness for each of the four identities and for rejection.
Two additional rejecting probes assert false claims about a longer word and an unknown keyword.
The proof imports the production keyword decision after token extraction.
Comment-line splitting, Unicode whitespace, and decorative-star handling use the standard library in the scanner adapter.
Native tests and the independent reference check those adapter semantics, language entry points, policies, and incremental edits.
The keyword proof supplies no formal verification of that adapter.

## TSX generic-arrow lookahead

The scanner uses the [TypeScript 5.9.3 lookahead rule](https://github.com/microsoft/TypeScript/blob/v5.9.3/src/compiler/parser.ts#L5341) to distinguish a generic arrow head from JSX.
After an optional leading `const` and a parameter name, a comma or equals token selects type parameters.
An `extends` token does the same unless its next token is equals, greater-than, or slash.
Those three tokens can belong to JSX attributes.

The source-bound goal proof covers all seven states of the production `AfterName` type.
It requires both lexical goals to be reachable and checks two separate false-claim probes.
The goal proof covers the decision after token extraction.
An additional source-bound proof checks the Unicode escape decoder against an independent hexadecimal scalar specification for every byte prefix of length 0 through 12.
It checks fixed-width and braced escapes, leading zeros, consumed width, malformed input, surrogate rejection, and the Unicode scalar limit.
Its unwind bound is 13, and separate false-claim probes require rejection of a surrogate and an out-of-range value.
The scalar syntax follows the [ECMAScript lexical grammar](https://tc39.es/ecma262/multipage/ecmascript-language-lexical-grammar.html#prod-UnicodeEscapeSequence).
This bounded decoder proof supplies no proof of Unicode identifier membership or of arbitrarily long escapes.
Tokenization, Unicode trivia, identifier classification, and full syntax validity remain separate integration checks.
The Rust and OCaml scanners implement token extraction independently.
An editor unit test also passes the shared regression sources to the locked TypeScript compiler with diagnostic reporting enabled.
It checks both accepted sources and rejected syntax through that independent parser.

## Commands and release gate

The normal `cargo xtask preflight` and pull-request Rust job run `cargo xtask proofs --check`.
This validates the inventory, input paths, pinned verifier, and harness syntax and formatting without running Kani.

On the documented Linux verification runner:

```sh
mise trust --quiet proofs/mise.toml
mise -C proofs x rust cargo:kani-verifier -- cargo kani setup
mise -C proofs x rust cargo:kani-verifier -- cargo run --locked --manifest-path ../rust/Cargo.toml -p xtask -- proofs
```

Owner machines use their bounded container runner for Kani.
The release metadata job runs the same project command before packaging can begin.
`cargo xtask release-check` also runs it after preflight.
The gate requires every declared harness, completed structured results, Kani 0.68.0 with CBMC 6.11.0, the x86_64 GNU Linux verification target, successful contract assertions and covers, and unchanged inputs.
It prints input SHA-256 digests and retains source snapshots and structured results under `rust/target/assurance/`.
It never reuses an earlier success receipt.
The per-harness 60-second stop is a resource safeguard and leaves unfinished work inconclusive.

## Limits and complementary checks

These are finite proofs of the imported Rust functions, not an unbounded proof of the scanner or a proof of OCaml.
They trust Kani, its compiler and solver, the lexical token boundaries, comment normalization, and the regular expression adapter.
They assume the local toolchain and filesystem are trusted during the check.
The receipt parser and filesystem adapter have ordinary success and refusal tests.
The independent OCaml implementation, recorded differential fixtures, explanation tests, and full-versus-incremental comparisons check observable integration behavior.
The block fixtures cover matching at the first, middle, and last member, separation by blank lines, code and comment kind, line ending variants, style precedence, and declared language profiles.
No passing test or differential comparison is counted as a formal proof.
Changing these pure decisions requires updating the inventory and affected harnesses; expanding the promised domain requires expanding the bounds or adding an unbounded argument.
The [decision record](adr/verify-comment-block-decisions.md) explains the verification boundary.
