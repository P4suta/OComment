# Governance

OComment is currently maintained by [@P4suta](https://github.com/P4suta), who is
responsible for repository administration, final technical decisions, and
release authorization.

Design work is discussed in public issues or Discussions whenever possible.
Decisions prioritize, in order, byte and file safety, compatibility with the
documented lexical contracts, deterministic Rust/OCaml agreement, correctness,
and measured performance. Significant changes should record the alternatives
and compatibility consequences before implementation.

Contributors can earn broader maintenance responsibility through sustained,
high-quality review and implementation work. Changes to governance will be made
through a pull request so the history remains public.

The active default-branch and release-tag protections are mirrored under
`.github/rulesets/`. `main` requires the portable CI matrix and squash merging;
the fixed-runner performance gate remains opt-in while that runner is offline.
