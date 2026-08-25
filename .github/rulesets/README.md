# Repository rulesets

These JSON files mirror the active GitHub repository rulesets and can be
imported from the repository rules settings page or sent to the repository
rulesets REST endpoint.

- `main.json` requires pull requests, immutable linear history, signed commits,
  resolved review threads, and every portable CI job.
- `release-tags.json` makes version tags immutable and requires their target
  commits to be signed.

The fixed-runner benchmark is intentionally not a required check because the
runner may be offline. It is enabled separately with the
`OCOMMENT_BENCHMARK_ENABLED` repository variable. Update the checked-in JSON in
the same pull request as any live ruleset change.
