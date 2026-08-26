#!/usr/bin/env python3
"""Fail when a name `spec/directives.toml` protects is not protected in fact.

`protected` in the shared spec is the list of markers that take a comment out
of reach of a `remove` policy. Nothing read that list until this ran, so the
spec and the scanner it describes could drift apart without a test noticing,
and the way that drift reaches a checkout is a build directive quietly deleted
by `ocomment fix`.

So every name is fed to the built binary as the one-line comment a project
would really write it as, and the answer has to be `keep` with the reason that
says why. Two more comments have to come back removable:

* an ordinary comment beside the marker, which catches a run that kept
  everything and would otherwise pass this check while removing nothing at all;
  and
* a near-miss, which catches the opposite mistake -- a marker matched so
  loosely that a comment merely *about* the tool is protected too.

The near-miss is written from the marker's own text rather than from the name
`spec/directives.toml` files it under: `# hadolintish note` says something
about `# hadolint ignore=`, where `# lint-and-formatterish note` would say
nothing about `// eslint-disable-next-line`. It is also scanned in the marker's
own place -- the same file with the marker line swapped out -- because half of
these markers are protected by where they sit as much as by what they say. A
shebang is a shebang only on the first line at the first byte, and an Oracle
hint only when its `+` touches the `/*`, so a near-miss appended to the end of
the file could never have been protected and would prove nothing about either
rule.

Where every way of running letters on past a marker is still that marker --
`//go:` is a namespace, and `//go:ish` is exactly the shape of a Go directive
-- the near-miss mentions the marker instead of opening with it, which is the
one thing the scanner still has to be able to tell apart.

Only the standard library is used, because this runs next to
`tools/check_hooks.py` in a job that installs nothing.
"""

from __future__ import annotations

import argparse
import dataclasses
import json
import pathlib
import re
import subprocess
import tomllib


ROOT = pathlib.Path(__file__).resolve().parents[1]
DIRECTIVES = ROOT / "spec/directives.toml"

# NOTE: The two `Keep` reasons the scanner gives a protected comment. A preamble
# NOTE: is held back by the file's own syntax and a directive by the tool that
# NOTE: reads it, and the report says which, so the samples below say it too.
KEPT_AS_PREAMBLE = "required source preamble"
KEPT_AS_DIRECTIVE = "tool or language directive"

# NOTE: Where the marker goes in a sample's template. It is substituted rather
# NOTE: than formatted, so a sample is free to contain braces of its own.
SLOT = "{}"

# NOTE: What a near-miss usually is: the marker with letters run straight on
# NOTE: past it, which is prose about the tool rather than an instruction to it.
# NOTE: A marker matched as a bare prefix keeps this by mistake.
NEGATIVE_SUFFIX = "ish note"


@dataclasses.dataclass(frozen=True)
class Sample:
    """One protected marker as a project would write it, and its controls.

    `template` is the file both scans are built from: `SLOT` is where the
    comment under test goes, and the ordinary comment after it is the one the
    scanner has to be willing to remove either way. `marker` is the directive
    itself and `near_miss` is the comment that must not be protected, which
    takes the marker's place so that the two differ in nothing but their text.
    """

    language: str
    dialect: str | None
    template: str
    marker: str
    near_miss: str
    reason: str

    def source(self, comment: str) -> bytes:
        """The sample as a file, with `comment` where the marker goes."""
        return self.template.replace(SLOT, comment, 1).encode()


# INVARIANT: One sample for each name in `spec/directives.toml`, and one name
# INVARIANT: for each sample -- `main` compares the two sets, so a name added to
# INVARIANT: the shared spec fails here until a sample proves the scanner knows
# INVARIANT: it. A name is a category (`shebang`, `lint-and-formatter`) as often
# INVARIANT: as it is a literal prefix, which is why the sample is written out
# INVARIANT: rather than derived from the name -- and why the near-miss beside
# INVARIANT: it is written from the marker rather than from the name too.
SAMPLES: dict[str, Sample] = {
    "shebang": Sample(
        "shell",
        None,
        f"{SLOT}\n# control\n",
        "#!/bin/sh",
        # NOTE: Every `#!` line at the first byte is a shebang, whatever
        # NOTE: interpreter follows, so running letters on past `/bin/sh` would
        # NOTE: still be one. What the rule also promises is that the `!`
        # NOTE: touches the `#`, and that is what the near-miss takes away.
        "# !/bin/shish note",
        KEPT_AS_PREAMBLE,
    ),
    "encoding": Sample(
        "python",
        None,
        f"{SLOT}\n# control\n",
        "# -*- coding: utf-8 -*-",
        "# -*- codingish: utf-8 -*-",
        KEPT_AS_PREAMBLE,
    ),
    "go:": Sample(
        "go",
        None,
        f"{SLOT}\n// control\n",
        "//go:build linux",
        # NOTE: `//go:` is a namespace: every Go directive is spelled
        # NOTE: `//go:<name>`, so `//go:ish` is exactly the shape of one and
        # NOTE: protecting it is right. What the marker still promises is that
        # NOTE: it opens the comment, so the near-miss mentions it instead.
        "// a note about go:build linux",
        KEPT_AS_DIRECTIVE,
    ),
    "+build": Sample(
        "go",
        None,
        f"{SLOT}\n// control\n",
        "// +build linux",
        "// a note about +build linux",
        KEPT_AS_DIRECTIVE,
    ),
    "triple-slash-reference": Sample(
        "typescript",
        None,
        f"{SLOT}\n// control\n",
        '/// <reference path="types.d.ts" />',
        # NOTE: The marker is a shape rather than a word: a `///` comment
        # NOTE: opening with `<` is a reference whatever element follows, so
        # NOTE: the boundary left to get wrong is the opener. Two slashes are
        # NOTE: an ordinary comment that happens to quote the directive.
        '// <reference path="types.d.ts" />',
        KEPT_AS_DIRECTIVE,
    ),
    "sourceMappingURL": Sample(
        "javascript",
        None,
        f"{SLOT}\n// control\n",
        "//# sourceMappingURL=bundle.js.map",
        f"//# sourceMappingURL{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "sourceURL": Sample(
        "javascript",
        None,
        f"{SLOT}\n// control\n",
        "//# sourceURL=bundle.js",
        f"//# sourceURL{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "#__PURE__": Sample(
        "javascript",
        None,
        f"const value = {SLOT} factory();\n// control\n",
        "/*#__PURE__*/",
        # NOTE: The annotation ends in its own delimiter, so there is no word
        # NOTE: boundary after it to get wrong; `#__PURE__ish` is still the
        # NOTE: bundler's marker with rubbish appended.
        "/* a note about #__PURE__ elsewhere */",
        KEPT_AS_DIRECTIVE,
    ),
    "@__PURE__": Sample(
        "javascript",
        None,
        f"const value = {SLOT} factory();\n// control\n",
        "/*@__PURE__*/",
        "/* a note about @__PURE__ elsewhere */",
        KEPT_AS_DIRECTIVE,
    ),
    "lint-and-formatter": Sample(
        "javascript",
        None,
        f"{SLOT}\n// control\n",
        "// eslint-disable-next-line no-eval",
        # NOTE: `eslint` is a namespace as much as `go:` is -- every rule of
        # NOTE: it is spelled `eslint-<something>` -- so the near-miss is again
        # NOTE: the comment that talks about the directive instead of being it.
        "// a note about eslint-disable-next-line",
        KEPT_AS_DIRECTIVE,
    ),
    "type-checker": Sample(
        "python",
        None,
        f"value = 1  {SLOT}\n# control\n",
        "# type: ignore",
        # NOTE: The marker is matched as a bare prefix, so what is left to get
        # NOTE: wrong is its front: `type: ignore` ends where the checker's own
        # NOTE: word ends, and prose that runs on past it is not addressed to
        # NOTE: the checker at all.
        "# typeish: ignore",
        KEPT_AS_DIRECTIVE,
    ),
    "optimizer-hint": Sample(
        "sql",
        "oracle",
        f"select {SLOT} 1 from dual; -- control\n",
        "/*+ index(t) */",
        # NOTE: The `+` has to touch the `/*`, which is the whole of what makes
        # NOTE: a hint a hint; a block comment that merely opens with one is an
        # NOTE: ordinary comment about the index.
        "/* + index(t) */",
        KEPT_AS_DIRECTIVE,
    ),
    "version-comment": Sample(
        "sql",
        "mysql",
        f"{SLOT} -- control\n",
        "/*!40101 SET NAMES utf8 */",
        "/* !40101 SET NAMES utf8 */",
        KEPT_AS_DIRECTIVE,
    ),
    "syntax=": Sample(
        "shell",
        None,
        f"{SLOT}\n# control\n",
        "# syntax=docker/dockerfile:1",
        # NOTE: BuildKit writes the frontend straight after the `=`, so the
        # NOTE: marker carries its own boundary and `syntax=ish` is the
        # NOTE: directive naming a frontend that does not exist.
        "# a note about syntax=docker/dockerfile:1",
        KEPT_AS_DIRECTIVE,
    ),
    "hadolint": Sample(
        "shell",
        None,
        f"{SLOT}\n# control\n",
        "# hadolint ignore=DL3018",
        f"# hadolint{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    ":schema": Sample(
        "toml",
        None,
        f"{SLOT}\n# control\n",
        "#:schema https://example.test/pyproject.json",
        # NOTE: Taplo writes the schema URL after whitespace, so the marker ends
        # NOTE: at a boundary and prose that runs letters on past it -- a note
        # NOTE: about schemas rather than the file's own -- is not the marker.
        f"#:schema{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "taplo:": Sample(
        "toml",
        None,
        f"{SLOT}\n# control\n",
        "# taplo: array_auto_expand = false",
        # NOTE: The colon is the marker's own boundary, so `taplo:ish` is still
        # NOTE: an instruction to the formatter -- one naming an option it does
        # NOTE: not have. What is left to get wrong is the front of it, which is
        # NOTE: what a comment merely mentioning the tool takes away.
        "# a note about taplo: array_auto_expand",
        KEPT_AS_DIRECTIVE,
    ),
    "---@diagnostic": Sample(
        "lua",
        None,
        f"{SLOT}\n-- control\n",
        "---@diagnostic disable-next-line: undefined-global",
        # NOTE: `---@` is a shape rather than a word: every annotation of the
        # NOTE: Lua language server is spelled that way, and running letters on
        # NOTE: past `diagnostic` would still be one of them. What the marker
        # NOTE: promises is that it opens the comment, so the near-miss is the
        # NOTE: comment that talks about the annotation instead -- written with
        # NOTE: two dashes, because a third would make it documentation, which
        # NOTE: this repository's own configuration keeps for a reason that has
        # NOTE: nothing to do with the marker under test.
        "-- a note about ---@diagnostic disable-next-line",
        KEPT_AS_DIRECTIVE,
    ),
    "luacheck:": Sample(
        "lua",
        None,
        f"{SLOT}\n-- control\n",
        "-- luacheck: ignore 212",
        f"-- luacheck{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "selene:": Sample(
        "lua",
        None,
        f"{SLOT}\n-- control\n",
        "-- selene: allow(unused_variable)",
        f"-- selene{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "stylua:": Sample(
        "lua",
        None,
        f"{SLOT}\n-- control\n",
        "-- stylua: ignore",
        f"-- stylua{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "luacov:": Sample(
        "lua",
        None,
        f"{SLOT}\n-- control\n",
        "-- luacov: disable",
        f"-- luacov{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "yaml-language-server:": Sample(
        "yaml",
        None,
        f"{SLOT}\n# control\n",
        "# yaml-language-server: $schema=https://example.test/schema.json",
        # NOTE: The colon is the marker's own boundary, so letters run on past
        # NOTE: it are still an instruction to the editor's YAML server. What is
        # NOTE: left to get wrong is the front of it, which is what a comment
        # NOTE: merely mentioning the server takes away.
        "# a note about yaml-language-server: $schema",
        KEPT_AS_DIRECTIVE,
    ),
    "yamllint": Sample(
        "yaml",
        None,
        f"{SLOT}\n# control\n",
        "# yamllint disable-line rule:line-length",
        f"# yamllint{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "renovate:": Sample(
        "yaml",
        None,
        f"{SLOT}\n# control\n",
        "# renovate: datasource=docker depName=alpine",
        "# a note about renovate: datasource",
        KEPT_AS_DIRECTIVE,
    ),
    "checkov:skip": Sample(
        "yaml",
        None,
        f"{SLOT}\n# control\n",
        "# checkov:skip=CKV_AWS_20:public by design",
        # NOTE: Checkov writes the rule straight after the `=`, so the marker
        # NOTE: carries its own boundary and what is left to get wrong is again
        # NOTE: whether it opens the comment.
        "# a note about checkov:skip=CKV_AWS_20",
        KEPT_AS_DIRECTIVE,
    ),
    "trivy:ignore": Sample(
        "yaml",
        None,
        f"{SLOT}\n# control\n",
        "# trivy:ignore:AVD-AWS-0089",
        "# a note about trivy:ignore:AVD-AWS-0089",
        KEPT_AS_DIRECTIVE,
    ),
    "nosec": Sample(
        "yaml",
        None,
        f"{SLOT}\n# control\n",
        "# nosec",
        f"# nosec{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "kics-scan": Sample(
        "yaml",
        None,
        f"{SLOT}\n# control\n",
        "# kics-scan ignore-line",
        f"# kics-scan{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "phpcs:": Sample(
        "php",
        None,
        f"<?php\n{SLOT}\n// control\n",
        "// phpcs:ignore Squiz.Commenting.FunctionComment",
        # NOTE: The colon is the marker's own boundary and the whole namespace
        # NOTE: is addressed with it -- `ignore`, `disable`, `enable`,
        # NOTE: `ignoreFile` -- so what is left to get wrong is the front of it,
        # NOTE: which is what running letters on past `phpcs` takes away.
        f"// phpcs{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "@phpstan-ignore": Sample(
        "php",
        None,
        f"<?php\n{SLOT}\n// control\n",
        "// @phpstan-ignore-next-line",
        # NOTE: `@phpstan-ignore` is a namespace: `-line`, `-next-line`, and the
        # NOTE: bare form with an identifier behind it are all spelled by
        # NOTE: running letters on past it, so protecting `@phpstan-ignoreish`
        # NOTE: is right. What the marker still promises is the `@` and that it
        # NOTE: opens the comment, so the near-miss mentions it instead.
        "// a note about @phpstan-ignore-next-line",
        KEPT_AS_DIRECTIVE,
    ),
    "@psalm-suppress": Sample(
        "php",
        None,
        f"<?php\n{SLOT}\n// control\n",
        "/** @psalm-suppress InvalidReturnType */",
        # NOTE: Psalm writes the issue it silences after whitespace, so the
        # NOTE: marker ends at a boundary and prose that runs letters on past it
        # NOTE: is a note about the checker rather than an instruction to it.
        # NOTE: The near-miss drops one star, because a documentation comment is
        # NOTE: kept by this repository's own configuration for a reason that has
        # NOTE: nothing to do with the marker under test.
        f"/* @psalm-suppress{NEGATIVE_SUFFIX} */",
        KEPT_AS_DIRECTIVE,
    ),
    "@codeCoverageIgnore": Sample(
        "php",
        None,
        f"<?php\n{SLOT}\n// control\n",
        "// @codeCoverageIgnoreStart",
        # NOTE: The three forms PHPUnit reads differ only in what runs on past
        # NOTE: the marker -- nothing, `Start`, `End` -- so a suffix is still the
        # NOTE: shape of one and the near-miss is again the comment that talks
        # NOTE: about the annotation instead of being it.
        "// a note about @codeCoverageIgnoreStart",
        KEPT_AS_DIRECTIVE,
    ),
    # NOTE: Ruby's three magic comments, which the interpreter reads out of the
    # NOTE: head of a file. Each carries its own boundary in the colon, so what
    # NOTE: is left to get wrong is the front of it -- which is what running
    # NOTE: letters on past the word takes away.
    "frozen_string_literal:": Sample(
        "ruby",
        None,
        f"{SLOT}\n# control\n",
        "# frozen_string_literal: true",
        f"# frozen_string_literal{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "warn_indent:": Sample(
        "ruby",
        None,
        f"{SLOT}\n# control\n",
        "# warn_indent: true",
        f"# warn_indent{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "shareable_constant_value:": Sample(
        "ruby",
        None,
        f"{SLOT}\n# control\n",
        "# shareable_constant_value: literal",
        f"# shareable_constant_value{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    # NOTE: The three tools every Ruby project runs. `rubocop:` and `standard:`
    # NOTE: are namespaces -- `disable`, `enable`, `todo` -- so letters run on
    # NOTE: past the colon are still an instruction to the linter, and the
    # NOTE: near-miss is again the comment that talks about the directive
    # NOTE: instead of being it.
    "rubocop:": Sample(
        "ruby",
        None,
        f"{SLOT}\n# control\n",
        "# rubocop:disable Style/Documentation",
        "# a note about rubocop:disable Style/Documentation",
        KEPT_AS_DIRECTIVE,
    ),
    "standard:": Sample(
        "ruby",
        None,
        f"{SLOT}\n# control\n",
        "# standard:disable Style/StringLiterals",
        "# a note about standard:disable Style/StringLiterals",
        KEPT_AS_DIRECTIVE,
    ),
    "typed:": Sample(
        "ruby",
        None,
        f"{SLOT}\n# control\n",
        "# typed: strict",
        "# a note about typed: strict",
        KEPT_AS_DIRECTIVE,
    ),
    "@schema": Sample(
        "yaml",
        None,
        f"{SLOT}\n# control\n",
        "# @schema type: string",
        # NOTE: The `@` is what tells the annotation from prose: `schema` on its
        # NOTE: own is a word any comment about a schema opens with, so the
        # NOTE: near-miss is the comment that mentions the annotation instead of
        # NOTE: being one.
        "# a note about @schema type",
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


def marker_word(marker: str) -> str:
    """The first word of a marker, past whatever punctuation opens it.

    `sourceMappingURL` of `//# sourceMappingURL=bundle.js.map`, `bin` of
    `#!/bin/sh`, `coding` of `# -*- coding: utf-8 -*-`. A near-miss is checked
    to have kept it, so one with nothing of the marker left in it -- the
    `lint-and-formatterish note` this file used to derive from the category
    name -- is refused rather than left to go on proving nothing.
    """
    match = re.search(r"[A-Za-z_][A-Za-z_0-9-]*", marker)
    return match.group() if match else ""


def scan(binary: pathlib.Path, sample: Sample, comment: str) -> list[dict]:
    """Every comment the binary reports for one built sample, in source order."""
    arguments = [str(binary), "scan", "--format", "json", "--language", sample.language]
    if sample.dialect is not None:
        arguments += ["--dialect", sample.dialect]
    completed = subprocess.run(
        arguments + ["-"], input=sample.source(comment), check=True, capture_output=True
    )
    document = json.loads(completed.stdout)
    return document["files"][0]["report"]["comments"]


def check_sample(binary: pathlib.Path, name: str, failures: list[str]) -> None:
    """Run one sample and record what the binary said if it is not protection."""
    sample = SAMPLES[name]
    where = f"`{name}` ({sample.language})"
    word = marker_word(sample.marker)
    if word and word.lower() not in sample.near_miss.lower():
        failures.append(
            f"{where}: the near-miss `{sample.near_miss}` keeps no word of"
            f" `{sample.marker}`, so it tests nothing about that marker"
        )
    comments = scan(binary, sample, sample.marker)
    if len(comments) != 2:
        failures.append(
            f"{where}: {len(comments)} comments found, expected 2:"
            f" {sample.source(sample.marker)!r}"
        )
        return
    protected, control = comments
    disposition = protected["disposition"]
    if disposition.get("action") != "keep":
        failures.append(f"{where}: `{sample.marker}` is {disposition}, expected a keep")
    elif disposition.get("reason") != sample.reason:
        failures.append(
            f"{where}: kept as {disposition.get('reason')!r}, expected {sample.reason!r}"
        )
    if control["disposition"].get("action") != "remove":
        failures.append(
            f"{where}: the ordinary comment beside it was kept too,"
            " so the run protected the file rather than the marker"
        )
    near_miss = scan(binary, sample, sample.near_miss)
    if len(near_miss) != 2:
        failures.append(
            f"{where}: {len(near_miss)} comments found in the near-miss, expected 2:"
            f" {sample.source(sample.near_miss)!r}"
        )
        return
    if near_miss[0]["disposition"].get("action") != "remove":
        failures.append(
            f"{where}: `{sample.near_miss}` was kept in the marker's own place,"
            " so the marker is matched loosely enough to protect a comment that"
            " is only about it"
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
        " and none of them protects the near-miss written in its place"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
