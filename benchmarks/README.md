# Benchmark baseline

`linux-x86_64.json` belongs to the fixed release runner identified in the file.
Refresh it only after reviewing an intentional performance change, using the
JSON printed by `tools/release_gate.py --skip-regression`. Pull requests are
rejected when throughput falls or latency/size grows by more than 5%.
