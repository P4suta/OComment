---
name: ocomment
description: >-
  Use OComment to check, explain, and safely edit source comments under the project's policy.
  Use for comment findings, partial staging, machine reports, and agent editing hooks.
---

# OComment

Use the target project's pinned or installed `ocomment` and inspect its version and relevant `--help` when uncertain.
When changing OComment itself, follow `AGENTS.md` and `CONTRIBUTING.md` and run the workspace binary rather than a stale installed copy.
The [agent manual](../../docs/agents.md) explains reports, hooks, traces, and exit status.
The [configuration manual](../../docs/configuration.md) defines policy and path selection.

Inspect before changing bytes:

```sh
ocomment check --format agent PATH
ocomment check --explain PATH
ocomment diff PATH
ocomment fix --dry-run PATH
```

Respect the repository's policy instead of changing it to pass a report.
Prefer expressing an invariant through types and executable behavior to adding prose about it.
Preserve necessary legal notices, safety arguments, API usage documentation, and directives.
Do not delete a promise or a safety argument merely because its text is a finding; resolve the underlying work or keep its necessary contract correctly.

Use `ocomment fix --tidy PATH` for policy-driven rewrites that leave removals unapplied.
Apply `ocomment fix PATH` only after reviewing the proposed removals.
Inspect the resulting diff and run the checks affected by the edit.
Never use `--force-invalid` or `--force-protected` as a routine way past a refusal.
An invalid scan can contain unestablished spans; those spans are not permission to remove the code underneath them.

For partial staging, use the documented index-aware `--staged` flow and review any index changes before recommitting.
Do not use Lefthook `stage_fixed` or stage the whole working-tree file to hide a staged rewrite.
Use `--index-only` only when its intentionally separate working-tree behavior is appropriate.

Use `--format agent` for instructions an agent reads and `--format json`, `jsonl`, or `sarif` when a consumer needs fields.
Findings and machine output go to stdout; summaries and diagnostics go to stderr.
`check` exits 0 for clean, 1 for findings, and 2 for invalid input or execution failure.
`diff` and `fix --dry-run` exit 1 when they propose a change; that is not an execution error.
Hook surfaces have their own documented host protocol; a decision in JSON is not interpreted from the ordinary `check` exit status.

Use `ocomment doctor`, `ocomment selftest`, or `--trace json` to investigate configuration, installation, or scanner behavior.
Keep private source and trace payloads out of public artifacts.
Install a client-specific editing hook only within the user's authorized configuration scope and preserve unrelated host settings.
Do not infer permission to edit, remove comments, or submit an approval from the mere presence of a hook.
