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
import os
import pathlib
import re
import subprocess
import tempfile
import tomllib

# NOTE: The binary reads a user configuration from `$XDG_CONFIG_HOME/ocomment/config.toml`,
# NOTE: which is a real setting on a real machine and is meant to reach every run.
# NOTE: A check that let this machine's through would be a check whose answer depends on whose machine it ran on; `tools/gen_docs.py` has pointed both variables at an empty directory since it was written, and this follows it.
def _isolated_environment() -> dict[str, str]:
    environment = dict(os.environ)
    empty = tempfile.mkdtemp(prefix="ocomment-no-user-config-")
    environment.update({"HOME": empty, "XDG_CONFIG_HOME": empty})
    return environment


ISOLATED = _isolated_environment()



ROOT = pathlib.Path(__file__).resolve().parents[1]
DIRECTIVES = ROOT / "spec/directives.toml"

# NOTE: The two `Keep` reasons the scanner gives a protected comment.
# NOTE: A preamble is held back by the file's own syntax and a directive by the tool that reads it, and the report says which, so the samples below say it too.
KEPT_AS_PREAMBLE = "required source preamble"
KEPT_AS_DIRECTIVE = "tool or language directive"
# NOTE: The third reason, and the only one a `remove` policy cannot overrule.
# NOTE: `spec/directives.toml` files these under `load_bearing`, and the two lists are checked against each other below so that a marker cannot be promoted in the spec without its sample saying what changed.
KEPT_AS_LOAD_BEARING = "required by the language or its build"

# NOTE: Where the marker goes in a sample's template.
# NOTE: It is substituted rather than formatted, so a sample is free to contain braces of its own.
SLOT = "{}"

# NOTE: What a near-miss usually is: the marker with letters run straight on past it, which is prose about the tool rather than an instruction to it.
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


# INVARIANT: One sample for each name in `spec/directives.toml`, and one name for each sample -- `main` compares the two sets, so a name added to the shared spec fails here until a sample proves the scanner knows it.
# INVARIANT: A name is a category (`shebang`, `lint-and-formatter`) as often as it is a literal prefix, which is why the sample is written out rather than derived from the name -- and why the near-miss beside it is written from the marker rather than from the name too.
SAMPLES: dict[str, Sample] = {
    "shebang": Sample(
        "shell",
        None,
        f"{SLOT}\n# control\n",
        "#!/bin/sh",
        # NOTE: Every `#!` line at the first byte is a shebang, whatever interpreter follows, so running letters on past `/bin/sh` would still be one.
        # NOTE: What the rule also promises is that the `!` touches the `#`, and that is what the near-miss takes away.
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
        # NOTE: `//go:` is a namespace: every Go directive is spelled `//go:<name>`, so `//go:ish` is exactly the shape of one and protecting it is right.
        # NOTE: What the marker still promises is that it opens the comment, so the near-miss mentions it instead.
        "// a note about go:build linux",
        KEPT_AS_LOAD_BEARING,
    ),
    "+build": Sample(
        "go",
        None,
        f"{SLOT}\n// control\n",
        "// +build linux",
        "// a note about +build linux",
        KEPT_AS_LOAD_BEARING,
    ),
    "triple-slash-reference": Sample(
        "typescript",
        None,
        f"{SLOT}\n// control\n",
        '/// <reference path="types.d.ts" />',
        # NOTE: The marker is a shape rather than a word: a `///` comment opening with `<` is a reference whatever element follows, so the boundary left to get wrong is the opener.
        # NOTE: Two slashes are an ordinary comment that happens to quote the directive.
        '// <reference path="types.d.ts" />',
        KEPT_AS_LOAD_BEARING,
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
        # NOTE: The annotation ends in its own delimiter, so there is no word boundary after it to get wrong; `#__PURE__ish` is still the bundler's marker with rubbish appended.
        "/* a note about #__PURE__ elsewhere */",
        KEPT_AS_LOAD_BEARING,
    ),
    "@__PURE__": Sample(
        "javascript",
        None,
        f"const value = {SLOT} factory();\n// control\n",
        "/*@__PURE__*/",
        "/* a note about @__PURE__ elsewhere */",
        KEPT_AS_LOAD_BEARING,
    ),
    "#__NO_SIDE_EFFECTS__": Sample(
        "javascript",
        None,
        f"{SLOT}\nexport function f() {{}}\n// control\n",
        "/*#__NO_SIDE_EFFECTS__*/",
        "/* a note about #__NO_SIDE_EFFECTS__ elsewhere */",
        KEPT_AS_LOAD_BEARING,
    ),
    "webpack": Sample(
        "javascript",
        None,
        f'const m = import({SLOT} "./m");\n// control\n',
        '/* webpackChunkName: "x" */',
        # NOTE: The same option with the colon taken out.
        # NOTE: A webpack option is the word, one more word, and a colon, so this is the marker right up to the byte that ends its name -- which is the byte worth getting wrong, and the one `/* webpackish prose */` would never have exercised.
        '/* webpackChunkName "x" */',
        KEPT_AS_LOAD_BEARING,
    ),
    "vite-ignore": Sample(
        "javascript",
        None,
        f"const m = import({SLOT} url);\n// control\n",
        "/* @vite-ignore */",
        # NOTE: The marker stands alone before the import expression, so it ends at whitespace and `@vite-ignoreish` is not it.
        "/* @vite-ignoreish */",
        KEPT_AS_LOAD_BEARING,
    ),
    "lint-and-formatter": Sample(
        "javascript",
        None,
        f"{SLOT}\n// control\n",
        "// eslint-disable-next-line no-eval",
        # NOTE: `eslint` is a namespace as much as `go:` is -- every rule of it is spelled `eslint-<something>` -- so the near-miss is again the comment that talks about the directive instead of being it.
        "// a note about eslint-disable-next-line",
        KEPT_AS_DIRECTIVE,
    ),
    "type-checker": Sample(
        "python",
        None,
        f"value = 1  {SLOT}\n# control\n",
        "# type: ignore",
        # NOTE: The marker is matched as a bare prefix, so what is left to get wrong is its front: `type: ignore` ends where the checker's own word ends, and prose that runs on past it is not addressed to the checker at all.
        "# typeish: ignore",
        KEPT_AS_DIRECTIVE,
    ),
    "optimizer-hint": Sample(
        "sql",
        "oracle",
        f"select {SLOT} 1 from dual; -- control\n",
        "/*+ index(t) */",
        # NOTE: The `+` has to touch the `/*`, which is the whole of what makes a hint a hint; a block comment that merely opens with one is an ordinary comment about the index.
        "/* + index(t) */",
        KEPT_AS_LOAD_BEARING,
    ),
    "version-comment": Sample(
        "sql",
        "mysql",
        f"{SLOT} -- control\n",
        "/*!40101 SET NAMES utf8 */",
        "/* !40101 SET NAMES utf8 */",
        KEPT_AS_LOAD_BEARING,
    ),
    "syntax=": Sample(
        "shell",
        None,
        f"{SLOT}\n# control\n",
        "# syntax=docker/dockerfile:1",
        # NOTE: BuildKit writes the frontend straight after the `=`, so the marker carries its own boundary and `syntax=ish` is the directive naming a frontend that does not exist.
        "# a note about syntax=docker/dockerfile:1",
        KEPT_AS_LOAD_BEARING,
    ),
    # NOTE: A mise file task's header, which mise and the `usage` library read case-sensitively from the raw line.
    # NOTE: Each near-miss is the same word in the case prose writes it in, which is the boundary the raw-byte match exists to hold.
    "MISE": Sample(
        "shell",
        None,
        f"{SLOT}\n# control\n",
        '#MISE description="Audit the signing posture"',
        "# mise installs the runtime this task needs",
        KEPT_AS_LOAD_BEARING,
    ),
    "USAGE": Sample(
        "javascript",
        None,
        f"{SLOT}\n// control\n",
        '//USAGE flag "--fix" help="Repair what the audit finds"',
        "// Usage: node audit.js [--fix]",
        KEPT_AS_LOAD_BEARING,
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
        # NOTE: Taplo writes the schema URL after whitespace, so the marker ends at a boundary and prose that runs letters on past it -- a note about schemas rather than the file's own -- is not the marker.
        f"#:schema{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "taplo:": Sample(
        "toml",
        None,
        f"{SLOT}\n# control\n",
        "# taplo: array_auto_expand = false",
        # NOTE: The colon is the marker's own boundary, so `taplo:ish` is still an instruction to the formatter -- one naming an option it does not have.
        # NOTE: What is left to get wrong is the front of it, which is what a comment merely mentioning the tool takes away.
        "# a note about taplo: array_auto_expand",
        KEPT_AS_DIRECTIVE,
    ),
    "---@diagnostic": Sample(
        "lua",
        None,
        f"{SLOT}\n-- control\n",
        "---@diagnostic disable-next-line: undefined-global",
        # NOTE: `---@` is a shape rather than a word: every annotation of the Lua language server is spelled that way, and running letters on past `diagnostic` would still be one of them.
        # NOTE: What the marker promises is that it opens the comment, so the near-miss is the comment that talks about the annotation instead -- written with two dashes, because a third would make it documentation, which this repository's own configuration keeps for a reason that has nothing to do with the marker under test.
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
        # NOTE: The colon is the marker's own boundary, so letters run on past it are still an instruction to the editor's YAML server.
        # NOTE: What is left to get wrong is the front of it, which is what a comment merely mentioning the server takes away.
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
        # NOTE: Checkov writes the rule straight after the `=`, so the marker carries its own boundary and what is left to get wrong is again whether it opens the comment.
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
        # NOTE: The colon is the marker's own boundary and the whole namespace is addressed with it -- `ignore`, `disable`, `enable`,
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
        # NOTE: `@phpstan-ignore` is a namespace: `-line`, `-next-line`, and the bare form with an identifier behind it are all spelled by running letters on past it, so protecting `@phpstan-ignoreish` is right.
        # NOTE: What the marker still promises is the `@` and that it opens the comment, so the near-miss mentions it instead.
        "// a note about @phpstan-ignore-next-line",
        KEPT_AS_DIRECTIVE,
    ),
    "@psalm-suppress": Sample(
        "php",
        None,
        f"<?php\n{SLOT}\n// control\n",
        "/** @psalm-suppress InvalidReturnType */",
        # NOTE: Psalm writes the issue it silences after whitespace, so the marker ends at a boundary and prose that runs letters on past it is a note about the checker rather than an instruction to it.
        # NOTE: The near-miss drops one star, because a documentation comment is kept by this repository's own configuration for a reason that has nothing to do with the marker under test.
        f"/* @psalm-suppress{NEGATIVE_SUFFIX} */",
        KEPT_AS_DIRECTIVE,
    ),
    "@codeCoverageIgnore": Sample(
        "php",
        None,
        f"<?php\n{SLOT}\n// control\n",
        "// @codeCoverageIgnoreStart",
        # NOTE: The three forms PHPUnit reads differ only in what runs on past the marker -- nothing, `Start`, `End` -- so a suffix is still the shape of one and the near-miss is again the comment that talks about the annotation instead of being it.
        "// a note about @codeCoverageIgnoreStart",
        KEPT_AS_DIRECTIVE,
    ),
    # NOTE: Ruby's three magic comments, which the interpreter reads out of the head of a file.
    # NOTE: Each carries its own boundary in the colon, so what is left to get wrong is the front of it -- which is what running letters on past the word takes away.
    "frozen_string_literal:": Sample(
        "ruby",
        None,
        f"{SLOT}\n# control\n",
        "# frozen_string_literal: true",
        f"# frozen_string_literal{NEGATIVE_SUFFIX}",
        KEPT_AS_LOAD_BEARING,
    ),
    "warn_indent:": Sample(
        "ruby",
        None,
        f"{SLOT}\n# control\n",
        "# warn_indent: true",
        f"# warn_indent{NEGATIVE_SUFFIX}",
        KEPT_AS_LOAD_BEARING,
    ),
    "shareable_constant_value:": Sample(
        "ruby",
        None,
        f"{SLOT}\n# control\n",
        "# shareable_constant_value: literal",
        f"# shareable_constant_value{NEGATIVE_SUFFIX}",
        KEPT_AS_LOAD_BEARING,
    ),
    # NOTE: The three tools every Ruby project runs.
    # NOTE: `rubocop:` and `standard:` are namespaces -- `disable`, `enable`, `todo` -- so letters run on past the colon are still an instruction to the linter, and the near-miss is again the comment that talks about the directive instead of being it.
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
    # NOTE: `zig fmt` is the only tool that reads a Zig comment, and it reads the whole phrase rather than a prefix of it (`Ast/Render.zig` compares the trimmed comment past `//` with `zig fmt: off` for equality), so letters run on past `off` turn nothing off and must not be protected.
    "zig fmt:": Sample(
        "zig",
        None,
        f"{SLOT}\n// control\n",
        "// zig fmt: off",
        f"// zig fmt: off{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "styler:": Sample(
        "r",
        None,
        f"{SLOT}\n# control\n",
        "# styler: off",
        # NOTE: The colon is the marker's own boundary, so `styler:ish` is still an instruction to the formatter -- one naming a state it does not have.
        # NOTE: What is left to get wrong is the front of it, which is what a comment merely mentioning the tool takes away.
        "# a note about styler: off",
        KEPT_AS_DIRECTIVE,
    ),
    "nocov": Sample(
        "r",
        None,
        f"{SLOT}\n# control\n",
        "# nocov start",
        # NOTE: `nocov` is the whole word covr looks for -- `start`, `end`, and nothing at all may follow it -- so letters run straight on past it are prose about the tool rather than an instruction to it.
        f"# nocov{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    # NOTE: `// @dart = 2.12` is the language version comment the Dart scanner reads itself, and it decides which version of the language the file is written in, so a removal that took it would change what the code below it means.
    # NOTE: The `@dart` has to be followed by `=` and a version,
    # NOTE: which is what the near-miss takes away.
    "@dart": Sample(
        "dart",
        None,
        f"{SLOT}\n// control\n",
        "// @dart = 2.12",
        f"// @dart{NEGATIVE_SUFFIX}",
        KEPT_AS_LOAD_BEARING,
    ),
    # NOTE: `dart_style` matches its two markers by equality on the whole comment rather than by prefix -- `front_end/piece_writer.dart` switches on `comment.text` against `// dart format off` -- so letters run on past `off` turn nothing off.
    # NOTE: Measured on `dart format` from SDK 3.13.2,
    # NOTE: which reformatted the near-miss and left the marker's region alone.
    "dart format": Sample(
        "dart",
        None,
        f"{SLOT}\n// control\n",
        "// dart format off",
        f"// dart format off{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    "ignore:": Sample(
        "dart",
        None,
        f"{SLOT}\n// control\n",
        "// ignore: unused_local_variable",
        # NOTE: The colon is the marker's own boundary, so `ignore:ish` is still an instruction to the analyzer -- one naming a diagnostic it does not have.
        # NOTE: What is left to get wrong is the front of it, which is what a comment merely mentioning the mechanism takes away.
        "// a note about ignore: unused_local_variable",
        KEPT_AS_DIRECTIVE,
    ),
    "ignore_for_file:": Sample(
        "dart",
        None,
        f"{SLOT}\n// control\n",
        "// ignore_for_file: unused_import",
        "// a note about ignore_for_file: unused_import",
        KEPT_AS_DIRECTIVE,
    ),
    # NOTE: SwiftPM reads the tools version out of the first line of a `Package.swift` before it reads the manifest at all, so a removal that took it would leave a package that no longer builds.
    # NOTE: The colon is the marker's own boundary, which is why the near-miss mentions the marker instead of opening with it.
    "swift-tools-version:": Sample(
        "swift",
        None,
        f"{SLOT}\n// control\n",
        "// swift-tools-version:5.9",
        "// a note about swift-tools-version:5.9",
        KEPT_AS_LOAD_BEARING,
    ),
    "swiftlint:": Sample(
        "swift",
        None,
        f"{SLOT}\n// control\n",
        "// swiftlint:disable force_cast",
        "// a note about swiftlint:disable force_cast",
        KEPT_AS_DIRECTIVE,
    ),
    "swiftformat:": Sample(
        "swift",
        None,
        f"{SLOT}\n// control\n",
        "// swiftformat:disable redundantSelf",
        "// a note about swiftformat:disable redundantSelf",
        KEPT_AS_DIRECTIVE,
    ),
    # NOTE: `swift-format` reads three spellings of this one -- the bare marker,
    # NOTE: the `-file` that widens it to the whole file, and a `:` and a rule name -- and its own regular expressions anchor at the end of each, so letters run straight on past the marker turn nothing off.
    # NOTE: Measured on
    # NOTE: `swift-format` 6.3.3, which left `let    a     = 1` unformatted under
    # NOTE: the marker and reformatted it under the near-miss.
    "swift-format-ignore": Sample(
        "swift",
        None,
        f"{SLOT}\nlet control = 1 // control\n",
        "// swift-format-ignore",
        f"// swift-format-ignore{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    # NOTE: Roslyn's own `BeginsWithAutoGeneratedComment` searches the comments in front of a file's first token for `<auto-generated` and exempts a file that carries one from every analyzer that opts out of generated code, so a removal that took it would light up the diagnostics the file was written to escape.
    # NOTE: That search is `contains` rather than a prefix, and it is followed, so the near-miss cannot run letters on past the marker: what it takes away instead is the `<` that makes the marker an XML tag rather than prose about generated code.
    "<auto-generated": Sample(
        "csharp",
        None,
        f"{SLOT}\n// control\n",
        "// <auto-generated/>",
        "// a note about auto-generated code",
        KEPT_AS_DIRECTIVE,
    ),
    "ReSharper": Sample(
        "csharp",
        None,
        f"{SLOT}\n// control\n",
        "// ReSharper disable once UnusedMember.Local",
        # NOTE: `disable` and `restore` are the two verbs the tool reads, and white space has to stand between them and its name, so a comment that mentions the instruction is the near-miss the rule still has to tell apart.
        "// a note about ReSharper disable once",
        KEPT_AS_DIRECTIVE,
    ),
    # NOTE: CSharpier matches this one on the whole text of a `//` comment rather than by prefix.
    # NOTE: Measured on `csharpier` 1.3.0, which left
    # NOTE: `int    a     =    1;` unformatted under the marker and reformatted
    # NOTE: it under `// csharpier-ignore some text`, under `//  csharpier-ignore` with a second space, and under the near-miss below.

    # NOTE: The tool tier of the languages whose entries were blank.
    # NOTE: Eclipse reads `$NON-NLS-n$` and stops reporting the string literal on that line as one that was never externalised; Checkstyle's suppression filter reads `CHECKSTYLE:OFF`; SonarQube reads `NOSONAR` in most of the languages it analyses; Eclipse and IntelliJ both read `@formatter:off`.
    # NOTE: Each near-miss is a comment that opens with the same letters and means nothing to the tool.
    "$non-nls": Sample(
        "java",
        None,
        f"{SLOT}\n// control\n",
        "//$NON-NLS-1$",
        "// a note about $NON-NLS-1$",
        KEPT_AS_DIRECTIVE,
    ),
    "checkstyle:": Sample(
        "java",
        None,
        f"{SLOT}\n// control\n",
        "// CHECKSTYLE:OFF",
        "// checkstyle is configured in the build file",
        KEPT_AS_DIRECTIVE,
    ),
    "nosonar": Sample(
        "java",
        None,
        f"{SLOT}\n// control\n",
        "// NOSONAR the cast is checked above",
        "// nosonarqube is what the product is called",
        KEPT_AS_DIRECTIVE,
    ),
    "formatter:": Sample(
        "java",
        None,
        f"{SLOT}\n// control\n",
        "// @formatter:off",
        "// formatters are configured elsewhere",
        KEPT_AS_DIRECTIVE,
    ),

    # NOTE: Python's two blank spots.
    # NOTE: `# pylint: disable=` turns one check off and `# pragma: no cover` takes the line out of the coverage report,
    # NOTE: which is the same job `# nocov` does for R and `@codeCoverageIgnore` for PHP.
    "pylint:": Sample(
        "python",
        None,
        f"{SLOT}\n# control\n",
        "# pylint: disable=invalid-name",
        "# pylint is run in the pipeline",
        KEPT_AS_DIRECTIVE,
    ),
    "pragma:": Sample(
        "python",
        None,
        f"{SLOT}\n# control\n",
        "# pragma: no cover",
        "# pragmatism about naming",
        KEPT_AS_DIRECTIVE,
    ),

    # NOTE: Perl::Critic is addressed and released by a phrase rather than by a prefix, so each near-miss is the phrase with a longer word in place of its last.
    "no critic": Sample(
        "perl",
        None,
        f"{SLOT}\n# control\n",
        "## no critic (ProhibitMagicNumbers)",
        "# no criticism intended",
        KEPT_AS_DIRECTIVE,
    ),
    "use critic": Sample(
        "perl",
        None,
        f"{SLOT}\n# control\n",
        "## use critic",
        "# use critical thinking",
        KEPT_AS_DIRECTIVE,
    ),

    # NOTE: Three more the survey asked for.
    # NOTE: cppcheck reads its own name as a prefix, so the near-miss is a comment that mentions it; staticcheck's two are named in full because `lint:` alone is also a note about linting; scalafmt reads its pair by equality.
    "cppcheck-suppress": Sample(
        "c",
        None,
        f"{SLOT}\n// control\n",
        "// cppcheck-suppress nullPointer",
        "// a note about cppcheck-suppress",
        KEPT_AS_DIRECTIVE,
    ),
    "lint:ignore": Sample(
        "go",
        None,
        f"{SLOT}\n// control\n",
        "//lint:ignore SA1000 the pattern is checked",
        "// lint: we should add one",
        KEPT_AS_DIRECTIVE,
    ),
    "format:": Sample(
        "scala",
        None,
        f"{SLOT}\n// control\n",
        "// format: off",
        "// format: off for now",
        KEPT_AS_DIRECTIVE,
    ),
    "csharpier-ignore": Sample(
        "csharp",
        None,
        f"{SLOT}\nvar control = 1; // control\n",
        "// csharpier-ignore",
        f"// csharpier-ignore{NEGATIVE_SUFFIX}",
        KEPT_AS_DIRECTIVE,
    ),
    # NOTE: scala-cli reads a directive line before it reads the manifest at all, and the directive is `//>` followed by a space and a name, of which `using` is the one that configures the build.
    # NOTE: The boundary after `using` is what tells the instruction from a comment that only opens with the same letters: `//> usingless` is not one, and neither is a comment that mentions the directive.
    "//> using": Sample(
        "scala",
        None,
        f"{SLOT}\n// control\n",
        "//> using scala \"3.3.0\"",
        "// a note about //> using scala",
        KEPT_AS_LOAD_BEARING,
    ),
    "@schema": Sample(
        "yaml",
        None,
        f"{SLOT}\n# control\n",
        "# @schema type: string",
        # NOTE: The `@` is what tells the annotation from prose: `schema` on its own is a word any comment about a schema opens with, so the near-miss is the comment that mentions the annotation instead of being one.
        "# a note about @schema type",
        KEPT_AS_DIRECTIVE,
    ),
}


def protected_names() -> tuple[list[str], list[str]]:
    """The two protection tiers the shared spec publishes, in its own order.

    `protected` is the tool tier a `remove` policy may take; `load_bearing` is
    the tier it may not, because the language or its build reads those as part
    of the program. The distinction is the whole point of this check, so a spec
    missing either list is an error rather than an empty tier that quietly
    tests nothing.
    """
    with DIRECTIVES.open("rb") as stream:
        table = tomllib.load(stream)
    tiers = []
    for key in ("protected", "load_bearing"):
        names = table.get(key)
        if not isinstance(names, list) or not names:
            raise SystemExit(f"{DIRECTIVES.relative_to(ROOT)} lists nothing under `{key}`")
        tiers.append(names)
    return tiers[0], tiers[1]


def check_language_survey(binary: pathlib.Path, failures: list[str]) -> None:
    """Every language the binary has must state what it holds in both tiers.

    A catalogue that lists only what exists cannot tell "this language has no
    load-bearing comment" from "nobody has looked at this language", and those
    are different claims. The two survey tables make the second one impossible
    to leave implicit: a language missing from one is a language that was added
    without the question being asked, and that is how a tier ends up covering
    seven languages out of thirty without anyone deciding it should.

    Both tiers are surveyed, because the tier that decides whether a default
    `fix` takes a comment out is the tool tier, and it went years with `java`
    and `perl` holding nothing while every other language named its linter's
    marker. Nothing said so, because only the other tier was being asked.

    The markers each entry names are checked against the tier itself, so an
    entry cannot drift into naming something the tier does not hold. The
    cross-language markers -- `eslint`, `noqa`, `nolint`, `NOSONAR`, the
    formatter pragmas -- are left out of the entries on purpose: they are
    recognised in every language, so naming them in each would turn the survey
    into a list that is the same everywhere and says nothing about any of them.
    """
    with DIRECTIVES.open("rb") as stream:
        table = tomllib.load(stream)

    listing = subprocess.run(
        [str(binary), "languages"],
        capture_output=True,
        check=True,
        text=True,
        env=ISOLATED,
    ).stdout.splitlines()
    languages = {line.split("\t", 1)[0] for line in listing[1:] if line.strip()}
    protected, load_bearing = protected_names()

    for key, tier, label in (
        ("load_bearing_by_language", load_bearing, "load-bearing tier"),
        ("protected_by_language", protected, "tool tier"),
    ):
        survey = table.get(key)
        if not isinstance(survey, dict):
            failures.append(f"{DIRECTIVES.relative_to(ROOT)} has no `[{key}]` table")
            continue
        for language in sorted(languages - set(survey)):
            failures.append(
                f"`{language}` is a built-in language but {DIRECTIVES.relative_to(ROOT)}"
                f" does not say what it holds in the {label}; add it to `[{key}]`,"
                " with an empty list if it holds nothing of its own"
            )
        for language in sorted(set(survey) - languages):
            failures.append(
                f"`{language}` is in `[{key}]` but is not a built-in language"
            )
        for language in sorted(set(survey) & languages):
            for marker in survey[language]:
                if marker not in tier:
                    failures.append(
                        f"`{language}` names `{marker}` in `[{key}]`, but"
                        f" `{key.removesuffix('_by_language')}` does not list it"
                    )


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


def scan(
    binary: pathlib.Path, sample: Sample, comment: str, policy: str | None = None
) -> list[dict]:
    """Every comment the binary reports for one built sample, in source order."""
    arguments = [str(binary), "scan", "--format", "json", "--language", sample.language]
    if policy is not None:
        arguments += ["--policy", policy]
    if sample.dialect is not None:
        arguments += ["--dialect", sample.dialect]
    completed = subprocess.run(
        arguments + ["-"],
        input=sample.source(comment),
        check=True,
        capture_output=True,
        env=ISOLATED,
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
    check_policy_all(binary, sample, where, failures)


def check_policy_all(
    binary: pathlib.Path, sample: Sample, where: str, failures: list[str]
) -> None:
    """Run the same sample under `--policy all` and hold the tier to its promise.

    This is the check the two tiers exist for. Under the default policy every
    marker in the spec is kept and the two are indistinguishable; `all` is
    where they part, and it is the run a project reaches for when it wants
    comments gone -- so it is also the run that used to delete a build
    constraint. A tool-tier marker has to go, because `all` said it would take
    every comment and a linter suppression is one. A load-bearing marker has to
    stay, because removing it would change what compiles or what the code does,
    and no policy is offered that choice.
    """
    comments = scan(binary, sample, sample.marker, policy="all")
    if len(comments) != 2:
        failures.append(
            f"{where}: {len(comments)} comments found under --policy all, expected 2"
        )
        return
    action = comments[0]["disposition"].get("action")
    # NOTE: A preamble is held back from `all` by the same force_protected gate as a load-bearing directive -- it is the older half of that gate -- so the two expect a keep and only the tool tier expects a removal.
    if sample.reason in (KEPT_AS_LOAD_BEARING, KEPT_AS_PREAMBLE):
        if action != "keep":
            failures.append(
                f"{where}: `{sample.marker}` is {sample.reason} and --policy all"
                f" {action}s it; the language or its build reads that comment, so"
                " a run that took it would change the code rather than a report"
                " about it"
            )
    elif action != "remove":
        failures.append(
            f"{where}: `{sample.marker}` is in the tool tier and --policy all"
            f" {action}s it; `all` promised to take every comment a tool merely"
            " reads, and a marker it holds back belongs under `load_bearing`"
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

    tool_tier, load_bearing = protected_names()
    names = tool_tier + load_bearing
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
    # INVARIANT: The tier a marker is filed under in the shared spec and the reason its sample expects are two spellings of one decision, so they are compared rather than both trusted.
    # INVARIANT: Moving a marker between the lists is meant to be a visible act: it changes what `--policy all` does to a real checkout.
    for name in sorted(set(load_bearing) & set(SAMPLES)):
        if SAMPLES[name].reason != KEPT_AS_LOAD_BEARING:
            failures.append(
                f"`{name}` is under `load_bearing` in {DIRECTIVES.relative_to(ROOT)}"
                f" but its sample expects {SAMPLES[name].reason!r}"
            )
    for name in sorted(set(tool_tier) & set(SAMPLES)):
        if SAMPLES[name].reason == KEPT_AS_LOAD_BEARING:
            failures.append(
                f"`{name}` expects {KEPT_AS_LOAD_BEARING!r} but"
                f" {DIRECTIVES.relative_to(ROOT)} files it under `protected`"
            )
    for name in names:
        if name in SAMPLES:
            check_sample(binary, name, failures)
    check_language_survey(binary, failures)

    if failures:
        print("\n".join(failures))
        return 1
    print(
        f"{len(names)} protected directives in spec/directives.toml are recognised"
        f" ({len(load_bearing)} of them load-bearing and out of reach of"
        " --policy all), and none of them protects the near-miss written in its"
        " place"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
