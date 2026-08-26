#!/usr/bin/env python3
"""Fail when a name `spec/directives.toml` protects is not protected in fact.

`protected` in the shared spec is the list of markers that take a comment out
of reach of a `remove` policy. Nothing read that list until this ran, so the
spec and the scanner it describes could drift apart without a test noticing,
and the way that drift reaches a checkout is a build directive quietly deleted
by `ocomment fix`.

So every name is fed to the built binary as the one-line comment a project
would really write it as, and the answer has to be `keep` with the reason that
says why. Each sample carries two more comments the scanner has to be willing
to remove:

* an ordinary comment, which catches a run that kept everything and would
  otherwise pass this check while removing nothing at all; and
* a negative control derived from the name itself -- `hadolint` against
  `hadolintish note` -- which catches the opposite mistake, a marker matched so
  loosely that prose merely opening with those letters is protected too. Where
  the name is a namespace rather than a word, and everything after it is by
  design part of the directive, the derived text would still be a directive and
  the sample names its own near-miss instead.

Only the standard library is used, because this runs next to
`tools/check_hooks.py` in a job that installs nothing.
"""

from __future__ import annotations

import argparse
import dataclasses
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

# NOTE: What a negative control is: the protected name with letters run straight
# NOTE: on past it, which is prose about the tool rather than an instruction to
# NOTE: it. A marker matched as a bare prefix keeps this by mistake.
NEGATIVE_SUFFIX = "ish note"


@dataclasses.dataclass(frozen=True)
class Sample:
    """One protected marker as a project would write it, and its controls.

    `source` opens with the marker and carries an ordinary comment after it.
    `comment` is how one more line comment is written in this language, which
    is what the negative control is appended as. `negative` overrides the text
    derived from the name for the markers that have no word boundary to test.
    """

    language: str
    dialect: str | None
    source: bytes
    reason: str
    comment: str
    negative: str | None = None


# INVARIANT: One sample for each name in `spec/directives.toml`, and one name
# INVARIANT: for each sample -- `main` compares the two sets, so a name added to
# INVARIANT: the shared spec fails here until a sample proves the scanner knows
# INVARIANT: it. A name is a category (`shebang`, `lint-and-formatter`) as often
# INVARIANT: as it is a literal prefix, which is why the sample is written out
# INVARIANT: rather than derived from the name.
SAMPLES: dict[str, Sample] = {
    "shebang": Sample(
        "shell", None, b"#!/bin/sh\n# control\n", KEPT_AS_PREAMBLE, "# {}"
    ),
    "encoding": Sample(
        "python",
        None,
        b"# -*- coding: utf-8 -*-\n# control\n",
        KEPT_AS_PREAMBLE,
        "# {}",
    ),
    "go:": Sample(
        "go",
        None,
        b"//go:build linux\n// control\n",
        KEPT_AS_DIRECTIVE,
        "// {}",
        # NOTE: `//go:` is a namespace: every Go directive is spelled
        # NOTE: `//go:<name>`, so `//go:ish` is exactly the shape of one and
        # NOTE: protecting it is right. What the marker still promises is that
        # NOTE: it opens the comment, so the near-miss mentions it instead.
        negative="a note about go:build linux",
    ),
    "+build": Sample(
        "go",
        None,
        b"// +build linux\n// control\n",
        KEPT_AS_DIRECTIVE,
        "// {}",
        negative="a note about +build linux",
    ),
    "triple-slash-reference": Sample(
        "typescript",
        None,
        b'/// <reference path="types.d.ts" />\n// control\n',
        KEPT_AS_DIRECTIVE,
        "// {}",
    ),
    "sourceMappingURL": Sample(
        "javascript",
        None,
        b"//# sourceMappingURL=bundle.js.map\n// control\n",
        KEPT_AS_DIRECTIVE,
        "// {}",
    ),
    "sourceURL": Sample(
        "javascript",
        None,
        b"//# sourceURL=bundle.js\n// control\n",
        KEPT_AS_DIRECTIVE,
        "// {}",
    ),
    "#__PURE__": Sample(
        "javascript",
        None,
        b"const value = /*#__PURE__*/ factory();\n// control\n",
        KEPT_AS_DIRECTIVE,
        "// {}",
        # NOTE: The annotation ends in its own delimiter, so there is no word
        # NOTE: boundary after it to get wrong; `#__PURE__ish` is still the
        # NOTE: bundler's marker with rubbish appended.
        negative="a note about #__PURE__ elsewhere",
    ),
    "@__PURE__": Sample(
        "javascript",
        None,
        b"const value = /*@__PURE__*/ factory();\n// control\n",
        KEPT_AS_DIRECTIVE,
        "// {}",
        negative="a note about @__PURE__ elsewhere",
    ),
    "lint-and-formatter": Sample(
        "javascript",
        None,
        b"// eslint-disable-next-line no-eval\n// control\n",
        KEPT_AS_DIRECTIVE,
        "// {}",
    ),
    "type-checker": Sample(
        "python",
        None,
        b"value = 1  # type: ignore\n# control\n",
        KEPT_AS_DIRECTIVE,
        "# {}",
    ),
    "optimizer-hint": Sample(
        "sql",
        "oracle",
        b"select /*+ index(t) */ 1 from dual; -- control\n",
        KEPT_AS_DIRECTIVE,
        "-- {}",
    ),
    "version-comment": Sample(
        "sql",
        "mysql",
        b"/*!40101 SET NAMES utf8 */ -- control\n",
        KEPT_AS_DIRECTIVE,
        "-- {}",
    ),
    "syntax=": Sample(
        "shell",
        None,
        b"# syntax=docker/dockerfile:1\n# control\n",
        KEPT_AS_DIRECTIVE,
        "# {}",
        # NOTE: BuildKit writes the frontend straight after the `=`, so the
        # NOTE: marker carries its own boundary and `syntax=ish` is the
        # NOTE: directive naming a frontend that does not exist.
        negative="a note about syntax=docker/dockerfile:1",
    ),
    "hadolint": Sample(
        "shell",
        None,
        b"# hadolint ignore=DL3018\n# control\n",
        KEPT_AS_DIRECTIVE,
        "# {}",
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


def negative_control(name: str, sample: Sample) -> str:
    """The near-miss text this sample's marker must not protect."""
    return sample.negative if sample.negative is not None else f"{name}{NEGATIVE_SUFFIX}"


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
    sample = SAMPLES[name]
    where = f"`{name}` ({sample.language})"
    negative = negative_control(name, sample)
    source = sample.source + (sample.comment.format(negative) + "\n").encode()
    comments = scan(binary, sample.language, sample.dialect, source)
    if len(comments) != 3:
        failures.append(f"{where}: {len(comments)} comments found, expected 3: {source!r}")
        return
    protected, control, near_miss = comments
    disposition = protected["disposition"]
    if disposition.get("action") != "keep":
        failures.append(f"{where}: {source!r} is {disposition}, expected a keep")
    elif disposition.get("reason") != sample.reason:
        failures.append(
            f"{where}: kept as {disposition.get('reason')!r}, expected {sample.reason!r}"
        )
    if control["disposition"].get("action") != "remove":
        failures.append(
            f"{where}: the ordinary comment after it was kept too,"
            " so the run protected the file rather than the marker"
        )
    if near_miss["disposition"].get("action") != "remove":
        failures.append(
            f"{where}: `{negative}` was kept, so the marker is matched as a bare"
            " prefix and protects prose that merely opens with it"
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
    print(
        f"{len(names)} protected directives in spec/directives.toml are recognised,"
        " and none of them protects its near-miss"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
