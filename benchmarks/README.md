# Benchmark baseline

`linux-x86_64.json` belongs to the fixed release runner identified in the file.
Refresh it only after reviewing an intentional performance change, using the
JSON printed by `tools/release_gate.py --skip-regression`. Pull requests are
rejected when throughput falls or latency/size grows by more than 5%.

The gate also exercises regex-configured many-small-file scans and dense
Human, JSON, and SARIF output. Absolute budgets cover self-scan latency, a
64 MiB quiet check, 40,000 Human findings, and 100,000 machine findings. Their
fixed-runner measurements are rounded conservatively in the baseline so the
5% regression comparison remains meaningful without turning scheduler noise
into a release failure.

The benchmark workflow remains skipped until a runner with the
`self-hosted`, `linux`, `x64`, and `ocomment-benchmark` labels is online and the
repository variable `OCOMMENT_BENCHMARK_ENABLED` is set to `true`. This avoids
leaving pull requests permanently queued when the fixed runner is unavailable.
