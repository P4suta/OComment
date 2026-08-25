#!/usr/bin/env python3
"""Enforce the reproducible OComment release size and performance contract."""

from __future__ import annotations

import argparse
import json
import pathlib
import shutil
import statistics
import subprocess
import sys
import tempfile
import time


ROOT = pathlib.Path(__file__).resolve().parents[1]


def timed(command: list[str], cwd: pathlib.Path | None = None) -> tuple[float, int]:
    started = time.perf_counter()
    completed = subprocess.run(
        command,
        cwd=cwd,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    )
    return time.perf_counter() - started, completed.returncode


def median_command(
    command: list[str], runs: int, accepted: set[int], cwd: pathlib.Path | None = None
) -> float:
    samples = []
    for _ in range(runs):
        elapsed, status = timed(command, cwd)
        if status not in accepted:
            raise RuntimeError(f"command exited {status}: {' '.join(command)}")
        samples.append(elapsed)
    return statistics.median(samples)


def throughput(
    executable: pathlib.Path, language: str, size_mib: int, iterations: int
) -> float:
    completed = subprocess.run(
        [str(executable), language, str(size_mib), str(iterations)],
        text=True,
        capture_output=True,
        check=True,
    )
    return float(json.loads(completed.stdout)["mib_per_second"])


def no_op_ratio(binary: pathlib.Path, typos: str) -> float:
    with tempfile.TemporaryDirectory(prefix="ocomment-release-gate-") as raw:
        root = pathlib.Path(raw)
        fragment = b"const value: usize = 42;\n"
        source = (fragment * ((2 * 1024 * 1024 // len(fragment)) + 1))[
            : 2 * 1024 * 1024
        ]
        for index in range(8):
            (root / f"clean-{index}.rs").write_bytes(source)
        ocomment = median_command(
            [str(binary), "check", ".", "--format", "json"], 7, {0}, root
        )
        reference = median_command([typos, "."], 7, {0}, root)
        return ocomment / reference


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--binary",
        type=pathlib.Path,
        default=ROOT / "rust/target/release/ocomment",
    )
    parser.add_argument(
        "--throughput-binary",
        type=pathlib.Path,
        default=ROOT / "rust/target/release/examples/throughput",
    )
    parser.add_argument(
        "--baseline",
        type=pathlib.Path,
        default=ROOT / "benchmarks/linux-x86_64.json",
    )
    parser.add_argument("--version-runs", type=int, default=31)
    parser.add_argument("--size-mib", type=int, default=32)
    parser.add_argument("--scan-runs", type=int, default=7)
    parser.add_argument("--skip-regression", action="store_true")
    args = parser.parse_args()

    binary = args.binary.resolve()
    scanner = args.throughput_binary.resolve()
    if not binary.is_file() or not scanner.is_file():
        parser.error("build the release CLI and throughput example first")

    metrics = {
        "version_median_ms": median_command(
            [str(binary), "--version"], args.version_runs, {0}
        )
        * 1000.0,
        "binary_mib": binary.stat().st_size / (1024.0 * 1024.0),
        "c_mib_per_second": throughput(
            scanner, "c", args.size_mib, args.scan_runs
        ),
        "javascript_mib_per_second": throughput(
            scanner, "javascript", args.size_mib, args.scan_runs
        ),
        "shell_mib_per_second": throughput(
            scanner, "shell", args.size_mib, args.scan_runs
        ),
    }
    typos = shutil.which("typos")
    if typos:
        metrics["typos_ratio"] = no_op_ratio(binary, typos)

    failures = []
    maxima = {
        "version_median_ms": 20.0,
        "binary_mib": 25.0,
        "typos_ratio": 1.5,
    }
    minima = {
        "c_mib_per_second": 500.0,
        "javascript_mib_per_second": 200.0,
        "shell_mib_per_second": 200.0,
    }
    for name, limit in maxima.items():
        if name in metrics and metrics[name] > limit:
            failures.append(f"{name}={metrics[name]:.3f} exceeds {limit}")
    for name, limit in minima.items():
        if metrics[name] < limit:
            failures.append(f"{name}={metrics[name]:.3f} is below {limit}")

    if not args.skip_regression:
        baseline = json.loads(args.baseline.read_text(encoding="utf-8"))
        for name, value in metrics.items():
            if name not in baseline.get("metrics", {}):
                continue
            reference = float(baseline["metrics"][name])
            if name.endswith("_per_second"):
                regressed = value < reference * 0.95
            else:
                regressed = value > reference * 1.05
            if regressed:
                failures.append(
                    f"{name}={value:.3f} regressed more than 5% from {reference:.3f}"
                )

    print(
        json.dumps(
            {"metrics": metrics, "failures": failures}, indent=2, sort_keys=True
        )
    )
    if failures:
        for failure in failures:
            print(f"release gate: {failure}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
