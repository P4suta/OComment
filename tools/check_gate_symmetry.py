#!/usr/bin/env python3
"""Fail when a test suite has only ever watched its gate pass.

A gate that has only been observed *accepting* is half a gate. The half nobody
watched is the half that silently stops working: a check whose predicate became
`true` for every input still passes every test that only ever fed it something
acceptable, and the run it was meant to stop sails through.

This repository has met that failure. `explanations_agree_with_the_scanner_over_
the_whole_branch_table` went on passing while three rules it had never been
shown were added, because nothing made it look at them. The answer there was a
mechanism -- an exhaustive destructuring that will not compile until a new field
is classified -- and a mechanism beats an audit every time. This is the audit
for what has no mechanism yet.

It is deliberately coarse. Judging whether one test needs a negative twin is a
reading, and six hundred such readings recorded in a file would be six hundred
guesses nobody checked. What is checkable is the suite: a file full of tests
that never once watch a refusal is a file that would go on passing if the thing
it tests stopped refusing anything, and that is worth failing over.

    python3 tools/check_gate_symmetry.py             check
    python3 tools/check_gate_symmetry.py --report    print the per-suite counts

The report is the useful half day to day: a suite whose ratio has fallen is
usually a suite that grew twenty tests of one shape.
"""

import argparse
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]

# NOTE: Every integration suite. Unit tests live beside the code they test and
# NOTE: are not read here: a `#[cfg(test)]` module is usually one function's
# NOTE: table, where this question does not arise.
SUITES = sorted(path for path in ROOT.glob("rust/*/tests/*.rs"))

TEST = re.compile(r"#\[test\]\s*(?:#\[[^\]]*\]\s*)*fn\s+([a-z0-9_]+)")

# NOTE: The shapes an observed refusal takes in this repository. A test that
# NOTE: contains none of these has never seen the thing it tests say no.
REFUSALS = (
    re.compile(r"\.is_err\(\)"),
    re.compile(r"unwrap_err|expect_err"),
    re.compile(r"#\[should_panic"),
    # NOTE: An exit code that is not success: 1 is findings, 2 is a failure.
    re.compile(r"status\(\)?\.code\(\), Some\([12]\)|code\(\), Some\([12]\)"),
    # NOTE: `assert!(!...)` and `prop_assert!(!...)`: it must not hold.
    re.compile(r"assert!\(\s*\n?\s*!"),
    re.compile(r"\.is_empty\(\),?\s*$", re.MULTILINE),
    re.compile(r"assert!\([^)]*is_none\(\)"),
    re.compile(r"panic!\("),
)


def suite_counts(path: pathlib.Path) -> tuple[int, int]:
    """How many tests a suite holds, and how many refusals it observes."""
    text = path.read_text(encoding="utf-8")
    tests = len(TEST.findall(text))
    refusals = sum(len(pattern.findall(text)) for pattern in REFUSALS)
    return tests, refusals


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--report", action="store_true", help="print the per-suite counts and stop"
    )
    arguments = parser.parse_args()

    rows = []
    for suite in SUITES:
        tests, refusals = suite_counts(suite)
        if tests == 0:
            continue
        rows.append((suite.relative_to(ROOT), tests, refusals))

    if arguments.report:
        width = max(len(str(name)) for name, _, _ in rows)
        for name, tests, refusals in rows:
            ratio = refusals / tests
            print(f"{str(name):{width}}  tests={tests:4}  refusals={refusals:4}  {ratio:5.2f} per test")
        return 0

    silent = [(name, tests) for name, tests, refusals in rows if refusals == 0]
    if silent:
        for name, tests in silent:
            print(
                f"{name}: {tests} test(s) and not one of them watches anything be"
                " refused",
                file=sys.stderr,
            )
        print(
            "\nA suite that only ever watches its gate pass would go on passing if"
            " the gate stopped refusing anything. Add the other direction: a case"
            " the thing under test must reject, an exit code that is not 0, a"
            " `should_panic`, or -- for a property -- a witness that the generator"
            " actually reached the case the property is about.",
            file=sys.stderr,
        )
        return 1

    total_tests = sum(tests for _, tests, _ in rows)
    total_refusals = sum(refusals for _, _, refusals in rows)
    print(
        f"{len(rows)} test suites, {total_tests} tests, {total_refusals} observed"
        " refusals; every suite watches its gate refuse something"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
