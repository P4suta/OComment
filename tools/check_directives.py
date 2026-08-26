#!/usr/bin/env python3
"""Fail when a name `spec/directives.toml` protects is not protected in fact.

`protected` in the shared spec is the list of markers that take a comment out
of reach of a `remove` policy. Nothing read that list until this ran, so the
spec and the scanner it describes could drift apart without a test noticing,
and the way that drift reaches a checkout is a build directive quietly deleted
by `ocomment fix`.

So every name is fed to the built binary as the one-line comment a project
would really write it as, and the answer has to be `keep` with the reason that
says why. Each sample carries an ordinary comment after the protected one,
which has to come back `remove`: a run that kept everything would otherwise
pass this check while removing nothing at all.

Only the standard library is used, because this runs next to
`tools/check_hooks.py` in a job that installs nothing.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import tomllib


ROOT = pathlib.Path(__file__).resolve().parents[1]
DIRECTIVES = ROOT / "spec/directives.toml"

# NOTE: The two `Keep` reasons the scanner gives a protected comment. A preamble
# NOTE: is held back by the file's own syntax and a directive by the tool that
# NOTE: reads it, and the report says which, so the samples below say it too.
KEPT_AS_PREAMBLE = "required source preamble"
KEPT_AS_DIRECTIVE = "tool or language directive"

# INVARIANT: One sample for each name in `spec/directives.toml`, and one name
# INVARIANT: for each sample -- `main` compares the two sets, so a name added to
# INVARIANT: the shared spec fails here until a sample proves the scanner knows
# INVARIANT: it. A name is a category (`shebang`, `lint-and-formatter`) as often
# INVARIANT: as it is a literal prefix, which is why the sample is written out
# INVARIANT: rather than derived from the name.
SAMPLES: dict[str, tuple[str, str | None, bytes, str]] = {
    "shebang": ("shell", None, b"#!/bin/sh\n# control\n", KEPT_AS_PREAMBLE),
    "encoding": (
        "python",
        None,
        b"# -*- coding: utf-8 -*-\n# control\n",
        KEPT_AS_PREAMBLE,
    ),
    "go:": ("go", None, b"//go:build linux\n// control\n", KEPT_AS_DIRECTIVE),
    "+build": ("go", None, b"// +build linux\n// control\n", KEPT_AS_DIRECTIVE),
    "triple-slash-reference": (
        "typescript",
        None,
        b'/// <reference path="types.d.ts" />\n// control\n',
        KEPT_AS_DIRECTIVE,
    ),
    "sourceMappingURL": (
        "javascript",
        None,
        b"//# sourceMappingURL=bundle.js.map\n// control\n",
        KEPT_AS_DIRECTIVE,
    ),
    "sourceURL": (
        "javascript",
        None,
        b"//# sourceURL=bundle.js\n// control\n",
        KEPT_AS_DIRECTIVE,
    ),
    "#__PURE__": (
        "javascript",
        None,
        b"const value = /*#__PURE__*/ factory();\n// control\n",
        KEPT_AS_DIRECTIVE,
    ),
    "@__PURE__": (
        "javascript",
        None,
        b"const value = /*@__PURE__*/ factory();\n// control\n",
        KEPT_AS_DIRECTIVE,
    ),
    "lint-and-formatter": (
        "javascript",
        None,
        b"// eslint-disable-next-line no-eval\n// control\n",
        KEPT_AS_DIRECTIVE,
    ),
    "type-checker": (
        "python",
        None,
        b"value = 1  # type: ignore\n# control\n",
        KEPT_AS_DIRECTIVE,
    ),
    "optimizer-hint": (
        "sql",
        "oracle",
        b"select /*+ index(t) */ 1 from dual; -- control\n",
        KEPT_AS_DIRECTIVE,
    ),
    "version-comment": (
        "sql",
        "mysql",
        b"/*!40101 SET NAMES utf8 */ -- control\n",
        KEPT_AS_DIRECTIVE,
    ),
    "syntax=": (
        "shell",
        None,
        b"# syntax=docker/dockerfile:1\n# control\n",
        KEPT_AS_DIRECTIVE,
    ),
    "hadolint ": (
        "shell",
        None,
        b"# hadolint ignore=DL3018\n# control\n",
        KEPT_AS_DIRECTIVE,
    ),
}


def protected_names() -> list[str]:
    """The `protected` list the shared spec publishes."""
    with DIRECTIVES.open("rb") as stream:
        table = tomllib.load(stream)
    names = table.get("protected")
    if not isinstance(names, list) or not names:
        raise SystemExit(f"{DIRECTIVES.relative_to(ROOT)} lists nothing under `protected`")
    return names


def scan(binary: pathlib.Path, language: str, dialect: str | None, source: bytes) -> list[dict]:
    """Every comment the binary reports for one sample, in source order."""
    arguments = [str(binary), "scan", "--format", "json", "--language", language]
    if dialect is not None:
        arguments += ["--dialect", dialect]
    completed = subprocess.run(
        arguments + ["-"], input=source, check=True, capture_output=True
    )
    document = json.loads(completed.stdout)
    return document["files"][0]["report"]["comments"]


def check_sample(binary: pathlib.Path, name: str, failures: list[str]) -> None:
    """Run one sample and record what the binary said if it is not protection."""
    language, dialect, source, reason = SAMPLES[name]
    where = f"`{name}` ({language})"
    comments = scan(binary, language, dialect, source)
    if len(comments) != 2:
        failures.append(f"{where}: {len(comments)} comments found, expected 2: {source!r}")
        return
    protected, control = comments[0], comments[1]
    disposition = protected["disposition"]
    if disposition.get("action") != "keep":
        failures.append(f"{where}: {source!r} is {disposition}, expected a keep")
    elif disposition.get("reason") != reason:
        failures.append(
            f"{where}: kept as {disposition.get('reason')!r}, expected {reason!r}"
        )
    if control["disposition"].get("action") != "remove":
        failures.append(
            f"{where}: the ordinary comment after it was kept too,"
            " so the run protected the file rather than the marker"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--binary",
        type=pathlib.Path,
        default=ROOT / "rust/target/debug/ocomment",
    )
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"CLI binary does not exist: {binary}")

    names = protected_names()
    failures: list[str] = []
    for name in sorted(set(names) - set(SAMPLES)):
        failures.append(
            f"`{name}` is protected by {DIRECTIVES.relative_to(ROOT)}"
            f" and has no sample; add one to {pathlib.Path(__file__).name}"
        )
    for name in sorted(set(SAMPLES) - set(names)):
        failures.append(
            f"`{name}` has a sample but {DIRECTIVES.relative_to(ROOT)} does not protect it"
        )
    for name in names:
        if name in SAMPLES:
            check_sample(binary, name, failures)

    if failures:
        print("\n".join(failures))
        return 1
    print(f"{len(names)} protected directives in spec/directives.toml are recognised")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
