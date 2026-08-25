# Benchmark baseline

`linux-x86_64.json` belongs to the fixed release runner identified in the file.
Refresh it only after reviewing an intentional performance change, using the
JSON printed by `tools/release_gate.py --skip-regression`. Pull requests are
rejected when throughput falls or latency/size grows by more than 5%.

The benchmark workflow remains skipped until a runner with the
`self-hosted`, `linux`, `x64`, and `ocomment-benchmark` labels is online and the
repository variable `OCOMMENT_BENCHMARK_ENABLED` is set to `true`. This avoids
leaving pull requests permanently queued when the fixed runner is unavailable.
