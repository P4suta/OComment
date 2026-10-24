# Comment block assurance

`keep_regex` protects every member of a contiguous comment block of the same kind when any member matches.
The [configuration contract](configuration.md) defines its boundaries and precedence.
`PreparedScanner::scan`, explanation output, and incremental scans use this decision before shape rules.
Style rules still apply to kept comments.

## Checked properties

The [proof inventory](https://github.com/P4suta/OComment/blob/main/proofs/inventory.toml) imports byte-for-byte snapshots of the actual Rust decision module and proof admission function.
It checks these bounded properties with Kani 0.68.0, pinned in [the proof tool configuration](https://github.com/P4suta/OComment/blob/main/proofs/mise.toml).

| Property | Input domain |
| --- | --- |
| Adjacency means whitespace containing exactly one logical line ending | Every byte sequence of length 0 through 16, including LF, CRLF, and CR |
| A member inherits the earliest configured match from exactly its connected run | Every sequence of 0 through 8 members, arbitrary join flags, and optional machine-sized pattern indices |
| Proof admission requires all six evidence facts | Every combination of the six Boolean facts |

Each nonempty block harness requires reachable kept and removed outcomes.
The empty block has one possible outcome and requires its own reachable empty witness.
The adjacency and admission harnesses each require reachable acceptance and refusal witnesses.
Every declared contract assertion must report success even when compiler-generated library assertions are proven unreachable.
Separate block harnesses cover each length from 0 through 8 while keeping member data and the selected index arbitrary.
Each nonempty case checks an arbitrary member, so it covers every index without repeating the specification for each index in one execution.
Unwind bounds are 17 for at most 16 gap bytes and the member count plus one for each block case, with unwinding assertions enabled.
Six separate rejecting probes assert specific false claims about boundaries, propagation, and admission.
The gate requires their named assertion failures rather than accepting an arbitrary panic, solver error, or unwinding failure.
These probes test the proof boundary; they are separate from the positive proofs.

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
They trust Kani, its compiler and solver, the lexical token boundaries, and the regular expression adapter.
They assume the local toolchain and filesystem are trusted during the check.
The receipt parser and filesystem adapter have ordinary success and refusal tests.
The independent OCaml implementation, recorded differential fixtures, explanation tests, and full-versus-incremental comparisons check observable integration behavior.
The block fixtures cover matching at the first, middle, and last member, separation by blank lines, code and comment kind, line ending variants, style precedence, and declared language profiles.
No passing test or differential comparison is counted as a formal proof.
Changing these pure decisions requires updating the inventory and affected harnesses; expanding the promised domain requires expanding the bounds or adding an unbounded argument.
The [decision record](adr/verify-comment-block-decisions.md) explains the verification boundary.
