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


def peak_rss_mib(
    command: list[str], runs: int, accepted: set[int], cwd: pathlib.Path
) -> float:
    """Median GNU time maximum RSS for an otherwise silenced command."""
    time_binary = pathlib.Path("/usr/bin/time")
    if not time_binary.is_file():
        raise RuntimeError("the fixed Linux runner needs /usr/bin/time for RSS gates")
    samples = []
    for _ in range(runs):
        with tempfile.NamedTemporaryFile(prefix="ocomment-rss-") as measured:
            completed = subprocess.run(
                [
                    str(time_binary),
                    "--quiet",
                    "--format=%M",
                    f"--output={measured.name}",
                    *command,
                ],
                cwd=cwd,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=False,
            )
            if completed.returncode not in accepted:
                raise RuntimeError(
                    f"command exited {completed.returncode}: {' '.join(command)}"
                )
            measured.seek(0)
            samples.append(float(measured.read().decode("ascii").strip()) / 1024.0)
    return statistics.median(samples)


def write_to_size(path: pathlib.Path, fragment: bytes, size: int) -> None:
    chunk = fragment * max(1, (1024 * 1024) // len(fragment))
    remaining = size
    with path.open("wb") as output:
        while remaining:
            part = chunk[:remaining]
            output.write(part)
            remaining -= len(part)


def cli_workloads(binary: pathlib.Path) -> dict[str, float]:
    metrics: dict[str, float] = {}
    metrics["self_scan_median_ms"] = (
        median_command(
            [str(binary), "check", ".", "--quiet"], 7, {0, 1}, ROOT
        )
        * 1000.0
    )

    with tempfile.TemporaryDirectory(prefix="ocomment-quiet-rss-") as raw:
        root = pathlib.Path(raw)
        write_to_size(
            root / "large.rs", b"let value: usize = 42;\n", 64 * 1024 * 1024
        )
        metrics["quiet_check_peak_rss_mib"] = peak_rss_mib(
            [str(binary), "check", "large.rs", "--quiet"], 3, {0}, root
        )

    with tempfile.TemporaryDirectory(prefix="ocomment-dense-") as raw:
        root = pathlib.Path(raw)
        human = root / "dense-human.rs"
        human.write_bytes(b"let value = 1; // removable\n" * 40_000)
        metrics["dense_human_median_ms"] = (
            median_command(
                [
                    str(binary),
                    "scan",
                    human.name,
                    "--format",
                    "human",
                    "--no-preview",
                    "--quiet",
                ],
                3,
                {0},
                root,
            )
            * 1000.0
        )
        machine = root / "dense-machine.rs"
        machine.write_bytes(b"let value = 1; // removable\n" * 100_000)
        metrics["dense_json_peak_rss_mib"] = peak_rss_mib(
            [str(binary), "check", machine.name, "--format", "json", "--quiet"],
            3,
            {1},
            root,
        )
        metrics["dense_sarif_peak_rss_mib"] = peak_rss_mib(
            [str(binary), "check", machine.name, "--format", "sarif", "--quiet"],
            3,
            {1},
            root,
        )

    with tempfile.TemporaryDirectory(prefix="ocomment-regex-files-") as raw:
        root = pathlib.Path(raw)
        (root / ".ocomment.toml").write_text(
            'version = 1\n[policy]\nkeep_regex = ["SPDX-License-Identifier"]\n'
            'remove_regex = ["removable|TODO"]\n',
            encoding="utf-8",
        )
        for index in range(2_000):
            (root / f"small-{index:04}.rs").write_bytes(
                b"let value = 1; // removable\n"
            )
        metrics["regex_many_files_median_ms"] = (
            median_command([str(binary), "check", ".", "--quiet"], 5, {1}, root)
            * 1000.0
        )
    return metrics


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
    metrics.update(cli_workloads(binary))
    typos = shutil.which("typos")
    if typos:
        metrics["typos_ratio"] = no_op_ratio(binary, typos)

    failures = []
    maxima = {
        "version_median_ms": 20.0,
        "binary_mib": 25.0,
        "typos_ratio": 1.5,
        "self_scan_median_ms": 17.3,
        "quiet_check_peak_rss_mib": 90.0,
        "dense_human_median_ms": 1000.0,
        "dense_json_peak_rss_mib": 100.0,
        "dense_sarif_peak_rss_mib": 100.0,
        "regex_many_files_median_ms": 1000.0,
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
