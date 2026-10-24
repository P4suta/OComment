---
date: 2026-10-24
---

# Verify comment block decisions on production Rust

## Status

Proposed.

## Context

Preserving a `keep_regex` match across a comment block introduces decisions about adjacency, connected runs, and precedence.
A proof of a separately written model could remain valid after the scanner implementation changes.
The independent OCaml reference supplies integration comparisons but does not establish a universal Rust property.
Ordinary pull-request checks must stay inexpensive, and release checks must refuse incomplete proof evidence.

## Decision

Isolate adjacency and run propagation in a pure Rust module and import its exact bytes into pinned Kani harnesses.
Keep the OCaml reference independent.
Use finite input bounds, explicit unwinding, reachable cover witnesses, and separate probes for false claims.
Use a typed result adapter that requires the expected harness identities, versions, completed checks, and unchanged input snapshots.
Import the adapter's pure admission function into its own proof harness.
Run inventory validation in ordinary preflight and pull-request CI and run the complete proof inventory before release packaging.
Document the domains, assumptions, and complementary checks in [comment block assurance](../assurance.md).

A detached Lean model was rejected for this boundary because it would need an additional refinement argument to connect Rust byte handling and run propagation to the theorem.
Integration tests alone were rejected because examples do not prove the bounded exhaustive properties.
Running the proof inventory on every pull request was rejected because the release gate owns the full campaign and ordinary CI already checks the affected behavior deterministically.

## Consequences

Changing the production decision module changes the exact source checked by the proof gate.
Wrong assertion failures, incomplete results, and stale inputs cannot receive success credit under the checked admission contract.
The Rust proof domain remains finite, and the lexer, regular expression adapter, receipt parser, filesystem, compiler, solver, and OCaml implementation retain explicit trust or test boundaries.
Maintainers must preserve those distinctions and extend the argument if the promised proof domain expands.
