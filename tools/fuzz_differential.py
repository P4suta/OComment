#!/usr/bin/env python3
"""Fuzz the Rust engine against the OCaml reference over the differential protocol.

`tools/differential.py` asks the two implementations the questions
`spec/fixtures/v1` already knows to ask. This asks them questions nobody thought
of: random byte strings built from the delimiters, escapes, quotes and directive
words the built-in scanners care about, under every policy, layout and
dialect. A source that makes the two answer differently is a divergence, and a
divergence is either a bug in one of them or a hazard the corpus is missing.

    python3 tools/fuzz_differential.py                       one seed, 2000 sources per language
    python3 tools/fuzz_differential.py --seed 1 --seed 2     two seeds
    python3 tools/fuzz_differential.py --cases 200           a quicker sweep

This is an on-demand check, not a CI gate: it is random, it takes minutes, and
what it finds belongs in `spec/fixtures/v1/hazards.json` as a named case before
it belongs in a pipeline. Build both drivers first, the way
`spec/fixtures/README.md` does:

    cargo build --manifest-path rust/Cargo.toml -p ocomment-core --example ref_driver --locked
    opam exec -- dune build --root ocaml bin/main.exe

The exit status is 0 when every response agreed. Otherwise the divergences are
collapsed into distinct signatures -- the shape of the disagreement, not the
source that produced it -- and one shrunken repro is printed per signature, so a
thousand instances of one bug are reported once.
"""

import argparse
import base64
import json
import pathlib
import random
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
RUST = ROOT / "rust/target/debug/examples/ref_driver"
OCAML = ROOT / "ocaml/_build/default/bin/main.exe"
LANGUAGES = ROOT / "spec/languages.toml"

DEFAULT_CASES = 2000

# NOTE: One pool for every language rather than a pool per language. A scanner
# NOTE: is most likely to go wrong on a delimiter it does not own -- a Lua long
# NOTE: bracket in a TOML file, a heredoc in Kotlin -- and mixing the alphabets
# NOTE: is what puts those in front of it. The groups are named rather than
# NOTE: labelled in comments, so a token added to one lands where it belongs.
COMMENT_MARKERS = [
    "//", "///", "//!", "/*", "*/", "/**", "/*!", "(*", "*)", "<!--", "-->",
    "--", "---", "#", "#!", "--[[", "]]", "--[=[", "]=]", "[[", "[=[",
]
LINE_STRUCTURE = ["\n", "\r\n", "\r", " ", " ", " ", "\t", "\x0b", "\x0c"]
QUOTES_AND_ESCAPES = [
    '"', "'", "`", "\\", "\\\\", '"""', "'''", "r\"", "r#\"", "\"#", "cr#\"",
    "br\"", "R\"(", ")\"", "u8\"", "L'", "$$", "N'", "@\"", "$tag$", "e\"",
    "q'[", "]'", "\\z", "\\x41", "\\u{41}", "\\ddd", "\\\n",
]
CODE_BYTES = [
    "{", "}", "(", ")", "[", "]", ";", "=", "+", "-", "<", ">", "/", "*",
    "!", "?", ":", ".", ",", "a", "b", "x", "r", "let", "fn", "def", "class",
    "SELECT", "$", "%", "&", "|", "^", "~", "0x", "1", "'a", "&'a",
]
TRANSLATION_PHASE = ["\\u0041", "\\u002f", "\\u005c", "u0027", "\\\r\n"]
DIRECTIVE_WORDS = [
    "rustfmt::skip", "shellcheck disable=SC1000", "shellcheck", "hadolint",
    "eslint-disable", "prettier-ignore", "region", "endregion", "noqa",
    ":schema", "taplo:", "luacheck:", "---@diagnostic", "go:generate",
    "yamllint", "@schema", "yaml-language-server:",
    "Copyright (c) 2020", "SPDX-License-Identifier: MIT", "@license", "NOTE:",
    "coding: utf-8", "pragma once",
]
MARKUP = [
    "<script>", "</script>", "<style>", "</style>", "<![CDATA[", "]]>",
    "<div>", "</div>", "<!DOCTYPE html>",
]
SHELL_STRUCTURE = ["<<EOF", "EOF", "${", "$(", ")", "case", "esac", "in", "|", "&&"]

# NOTE: The shapes a YAML block scalar needs to open at all. Its body is the one
# NOTE: lexical state a single byte cannot reach: the header wants an indicator
# NOTE: where a node may begin and then the end of the line, and the body wants
# NOTE: a line indented past the node, so a per-token pool without these opens
# NOTE: one about as often as it opens a Lua long bracket without `--[[`.
# NOTE: `key:\n` and a bare `-` on its own line put the node that owns a body on
# NOTE: an earlier line than the header, which is the one thing about a YAML line
# NOTE: the line itself does not say; `!!str` and `&a` are the node properties
# NOTE: that may stand between the two, and `|+` the chomping that makes the
# NOTE: blank lines under a body content.
YAML_STRUCTURE = [
    "key: ", "- ", ": |", ": >-", "|2-", "|+", "\n  ", "%YAML 1.2", "...",
    "key:\n", "\n-\n", "!!str ", "&a ",
]

# NOTE: The shapes that reach PHP mode at all. Nothing but a whole `<?php` with
# NOTE: white space behind it opens one, so a per-byte pool would leave every
# NOTE: generated PHP source inline HTML and scan nothing; the heredoc header
# NOTE: and its terminator are here for the same reason the YAML ones are.
PHP_STRUCTURE = [
    "<?php ", "<?=", "?>", "<?xml ", "#[", "<<<EOT", "<<<'NOW'", "EOT;", "NOW;",
    "{$a}", "${a}", "phpcs:ignore", "@phpstan-ignore-next-line",
]

# NOTE: The shapes Ruby needs before a generated source reaches its lexical
# NOTE: states at all. Four of Ruby's tokens are spelled with a byte that is also
# NOTE: an operator, so the pool carries the whole opener rather than the byte:
# NOTE: a percent literal with each kind of delimiter, a here document header
# NOTE: with the terminator line that ends one, an interpolation, a character
# NOTE: literal, and a regular expression. `=begin` and `__END__` are here for
# NOTE: the reason the YAML headers are -- both are whole words at column zero
# NOTE: that a per-byte pool would practically never assemble. The last three
# NOTE: are the interpolation boundary a here document header may be written
# NOTE: across: the body such a header asks for belongs to the line the header
# NOTE: stands on, not to the interpolation, and only a pool that can assemble
# NOTE: an opener inside `"#{ ... }"` puts that in front of the two scanners.
RUBY_STRUCTURE = [
    "%w[", "%q(", "%r{", "%Q{", "<<~EOS", "<<EOS", "EOS", "?c", "?\\", "=begin",
    "=end", "__END__", "#{", "/re/", "$\"", "@a", ":sym", "empty?", "puts ",
    "\"#{", "}\"", "#{ <<EOS }",
]

# NOTE: The shapes Zig needs. It is the one built-in language whose multiline
# NOTE: string has no quote in it at all: a `\\` wherever a token may begin runs
# NOTE: to the end of that line as content, so a per-byte pool would open one
# NOTE: about as often as it opens a Lua long bracket. The slash runs are the
# NOTE: three comment markers and the fourth slash that takes the doc marker
# NOTE: back, `@"` the quoted identifier that is lexed as a string, and the two
# NOTE: `zig fmt` phrases the only directive it has.
ZIG_STRUCTURE = [
    "\\\\", "////", "///", "//!", "@\"", "zig fmt: off", "zig fmt: on", "'\\''",
]

# NOTE: The shapes R needs. Its raw string opens on a letter no other language
# NOTE: uses as a delimiter, so a pool without `r"(` never reaches the one
# NOTE: literal in the language that takes no escapes; the dashed pair is here
# NOTE: for the reason Lua's levelled brackets are, since a closing run of the
# NOTE: wrong length is content. `%in%` and the bare `%` are the operator whose
# NOTE: name may hold a `#`, `#'` is roxygen2's marker, and the two markers
# NOTE: after them are the directives styler and covr read.
R_STRUCTURE = [
    "r\"(", ")\"", "r\"--(", ")--\"", "R\"[", "]\"", "r\"{", "}\"", "R'(", ")'",
    "#'", "%in%", "%", "`", "styler: off", "nocov start", "xr\"(",
]

# NOTE: The shapes Dart needs. Its raw string opens on a letter, and only
# NOTE: where that letter begins a token, so the pool carries the two near
# NOTE: misses `xr'` and `1r'` beside the opener itself. `${` is what turns
# NOTE: the inside of a string back into code, `'''` is the multiline form a
# NOTE: per-byte alphabet reaches only by coincidence, and the slash runs are
# NOTE: the two doc markers Dart honours and the two it does not. The last
# NOTE: three are the instructions a Dart tool reads, two of them matched as
# NOTE: whole phrases rather than as prefixes.
DART_STRUCTURE = [
    "r'", "r\"", "xr'", "1r'", "${", "$a", "#foo", "//!", "/*!", "////",
    "'''", "// @dart = 2.12", "// dart format off", "ignore_for_file:",
]

# NOTE: The shapes Swift needs. A run of `#` renames both the delimiter and the
# NOTE: escape of a raw string, so the pool carries one-hash and two-hash
# NOTE: openers with the closers and the `\#(` that opens an interpolation
# NOTE: inside one; `#"""#` is the shape that reads as two things at once.
# NOTE: `#/` and `/#` delimit the regular expression literal that may hold an
# NOTE: unescaped `/` and may span lines, and `/a\//` is the bare literal whose
# NOTE: last two bytes spell `//`. The apostrophe is the delimiter the language
# NOTE: does not have and the compiler lexes anyway, and the last three are the
# NOTE: instructions a Swift tool reads.
SWIFT_STRUCTURE = [
    "#\"", "\"#", "##\"", "\"##", "#\"\"\"", "\"\"\"#", "#\"\"\"#", "\\#(", "\\(",
    "#/", "/#", "##/", "/##", "/a\\//", "a /b/ c", "'", "#if", "#warning(",
    "// swift-tools-version:5.9", "// swiftlint:disable all",
    "// swift-format-ignore",
]

# NOTE: The shapes C# needs. Its eight string forms are opened by a run of "$"
# NOTE: and "@" in front of a run of quotes, and which rule applies turns on the
# NOTE: length of both runs, so the pool carries the openers whole rather than
# NOTE: the bytes: "@\"" and "$@\"" carry line breaks, "$\"" carries a hole that
# NOTE: may, and "$$\"\"\"" needs two braces to open one where a single "{" is
# NOTE: content. "{{" and "}}" are an escape in one form and content in another,
# NOTE: and "u8" is the suffix that follows a closing quote. The directive words
# NOTE: after them are the lines lexed by rules of their own -- four of them take
# NOTE: the rest of the line as a message and the rest lex a string and a "//" --
# NOTE: and the last three are the instructions a C# tool reads. The two Unicode
# NOTE: escapes are the line terminators ECMA-334 counts and this repository's
# NOTE: other C-family scanners do not.
CSHARP_STRUCTURE = [
    "@\"", "$\"", "$@\"", "@$\"", "$$\"\"\"", "$\"\"\"", "\"\"\"\"", "{{", "}}", "u8",
    "@class", "$$", "\u2028", "\u0085",
    "#if ", "#endif", "#region ", "#endregion", "#error ", "#warning ",
    "#pragma warning disable ", "#line 1 ", "#nullable enable", "#!",
    "// <auto-generated/>", "// ReSharper disable once X", "// csharpier-ignore",
]

SCALA_STRUCTURE = [
    "s\"", "f\"", "raw\"", "xml\"", "\"\"\"", "\"\"\"\"", "\"\"\"\"\"", "$$", "$\"",
    "${", "$x", "$_", "`a//b`", "//> using scala ", "//> using", "return\"",
    "<a>", "</a>", "<a/>", "<!--", "-->", "<![CDATA[", "]]>", "<?", "?>", "x <",
    "> <", "\\u0022", "'c'", "'/", "'sym",
]

VUE_STRUCTURE = [
    "{{", "}}", "<!--", "-->", "<template>", "</template>", "<script", "</script>",
    "<style", "</style>", 'lang="ts"', 'lang="scss"', 'lang="less"', "v-pre",
    "<div", "</div>", ":title=", "// text", "/* c */",
]

SVELTE_STRUCTURE = [
    "{", "}", "{#if", "{/if}", "{#each", "{/each}", "<!--", "-->",
    "<script", "</script>", "<style", "</style>", 'lang="ts"', 'lang="scss"',
    "<p>", "</p>", "title=", "// text", "/* c */",
]

SCSS_STRUCTURE = [
    "//", "/*", "*/", "#{", "}", "url(", ")", "$x:", 'content: "', "//cdn/",
    "// c", "/* c */", "a {", "}",
]

# NOTE: The bytes a lexer is liable to mishandle: NUL, DEL, a byte order mark, a
# NOTE: no-break space, the two Unicode line terminators, and two characters
# NOTE: wider than one byte.
AWKWARD_BYTES = [
    "\x00", "\x7f", "﻿", " ", " ", " ", "é", "中",
]

TOKENS = (
    COMMENT_MARKERS
    + LINE_STRUCTURE
    + QUOTES_AND_ESCAPES
    + CODE_BYTES
    + TRANSLATION_PHASE
    + DIRECTIVE_WORDS
    + MARKUP
    + SHELL_STRUCTURE
    + YAML_STRUCTURE
    + PHP_STRUCTURE
    + RUBY_STRUCTURE
    + ZIG_STRUCTURE
    + R_STRUCTURE
    + DART_STRUCTURE
    + SWIFT_STRUCTURE
    + CSHARP_STRUCTURE
    + SCALA_STRUCTURE
    + VUE_STRUCTURE
    + SVELTE_STRUCTURE
    + SCSS_STRUCTURE
    + AWKWARD_BYTES
)

INVALID_UTF8 = [b"\xff", b"\xc0\xa0", b"\xed\xa0\x80", b"\xf5\x80\x80\x80"]

OPERATIONS = ["scan", "transform"]
POLICIES = ["safe", "all", "legal"]
LAYOUTS = ["lines", "columns", "compact"]


def built_in_languages():
    """The languages and dialects `spec/languages.toml` declares, so the sweep
    cannot fall behind a language someone adds."""
    table = tomllib.loads(LANGUAGES.read_text(encoding="utf-8"))
    return [(entry["name"], entry["dialects"]) for entry in table["languages"]]


def random_tokens(rng):
    """One source as the list of tokens it was built from, so it can be shrunk."""
    tokens = [rng.choice(TOKENS) for _ in range(rng.randint(1, 24))]
    if rng.random() < 0.1:
        tokens[rng.randrange(len(tokens))] = rng.choice(INVALID_UTF8)
    return tokens


def assemble(tokens):
    """The bytes a token list stands for."""
    return b"".join(
        token if isinstance(token, bytes) else token.encode("utf-8") for token in tokens
    )


def request(identifier, language, source, options, operation):
    """One protocol request, in the shape `spec/differential-protocol.md` fixes."""
    return {
        "id": identifier,
        "operation": operation,
        "language": language,
        "source_base64": base64.b64encode(source).decode(),
        "options": options,
    }


def random_options(rng, dialects):
    """Policy knobs for one request; `layout` is read only by `transform`."""
    options = {"policy": rng.choice(POLICIES), "layout": rng.choice(LAYOUTS)}
    if len(dialects) > 1:
        options["dialect"] = rng.choice(dialects)
    if rng.random() < 0.1:
        options["force_invalid"] = True
    if rng.random() < 0.1:
        options["force_protected"] = True
    return options


def run(executable, requests):
    """Feed a batch to one implementation and parse the responses."""
    payload = "".join(json.dumps(item, separators=(",", ":")) + "\n" for item in requests)
    completed = subprocess.run(
        [str(executable)], input=payload, text=True, capture_output=True, check=True
    )
    return [json.loads(line) for line in completed.stdout.splitlines()]


def compare(requests):
    """Every request of the batch whose two responses differ, as
    `(request, rust, ocaml)`."""
    rust = run(RUST, requests)
    ocaml = run(OCAML, requests)
    if not len(rust) == len(ocaml) == len(requests):
        raise SystemExit(
            f"response count {len(rust)} (rust) vs {len(ocaml)} (ocaml) "
            f"for {len(requests)} request(s)"
        )
    return [
        (item, left, right)
        for item, left, right in zip(requests, rust, ocaml)
        if left != right
    ]


def difference_paths(left, right, prefix=""):
    """Where two responses differ, as dotted paths with list indices collapsed.

    Collapsing the indices is what makes a signature: the same bug reached from
    a hundred sources names the same fields, however many comments happened to
    precede the one it went wrong on.
    """
    if type(left) is not type(right):
        return [f"{prefix}:type"]
    if isinstance(left, dict):
        paths = []
        for key in sorted(set(left) | set(right)):
            if key not in left or key not in right:
                paths.append(f"{prefix}.{key}:absent")
            else:
                paths.extend(difference_paths(left[key], right[key], f"{prefix}.{key}"))
        return paths
    if isinstance(left, list):
        if len(left) != len(right):
            return [f"{prefix}:length"]
        paths = []
        for item, other in zip(left, right):
            paths.extend(difference_paths(item, other, f"{prefix}[]"))
        return sorted(set(paths))
    return [] if left == right else [prefix]


def signature(language, left, right):
    """What tells one divergence from another: the language, the fields that
    disagree, and -- where the field is a message or a kind -- the two values,
    because those name the rule that went wrong."""
    paths = tuple(sorted(set(difference_paths(left, right))))
    named = []
    for path in paths:
        if path.endswith((".message", ".kind", ".code", ".action", ".reason")):
            named.append((path, extract(left, path), extract(right, path)))
    return (language, paths, tuple(named))


def extract(value, path):
    """The value at a dotted path, or `None` where the path does not lead
    anywhere -- a list index was collapsed, or a key is absent."""
    for step in path.lstrip(".").split("."):
        if step.endswith("[]"):
            step = step[:-2]
            if not isinstance(value, dict) or step not in value:
                return None
            value = value[step]
            if not isinstance(value, list) or not value:
                return None
            value = value[0]
        elif isinstance(value, dict) and step in value:
            value = value[step]
        else:
            return None
    return value


def shrink(item, tokens, language, budget):
    """A shorter token list that still diverges, by dropping one token at a time.

    Greedy and bounded per signature: a divergence is meant to be rare, and the
    point of the repro is to be short enough to paste into a fixture `note`,
    not to be minimal. Every pass over the list is repeated until one removes
    nothing, so a token that only became removable after its neighbour went is
    still reached.
    """
    current = list(tokens)
    probes = budget
    changed = True
    while changed and probes > 0:
        changed = False
        index = 0
        while index < len(current) and probes > 0:
            candidate = current[:index] + current[index + 1 :]
            if not candidate:
                break
            probes -= 1
            probe = request(item["id"], language, assemble(candidate), item["options"],
                            item["operation"])
            if compare([probe]):
                current = candidate
                changed = True
            else:
                index += 1
    # NOTE: The two answers are read back from the shrunken source, so what the
    # NOTE: report prints is what the source it prints really produces.
    probe = request(item["id"], language, assemble(current), item["options"],
                    item["operation"])
    _, left, right = compare([probe])[0]
    return assemble(current), left, right


def sweep(seed, cases, languages):
    """Every divergence one seed turns up, keyed by signature."""
    rng = random.Random(seed)
    requests = []
    sources = {}
    for language, dialects in languages:
        for index in range(cases):
            tokens = random_tokens(rng)
            identifier = f"{language}-{seed}-{index}"
            sources[identifier] = tokens
            requests.append(
                request(
                    identifier,
                    language,
                    assemble(tokens),
                    random_options(rng, dialects),
                    rng.choice(OPERATIONS),
                )
            )
    return requests, sources, compare(requests)


def main(argv):
    parser = argparse.ArgumentParser(
        description="Fuzz the Rust engine against the OCaml reference.",
        epilog="An on-demand check. What it finds belongs in spec/fixtures/v1/hazards.json.",
    )
    parser.add_argument(
        "--seed", type=int, action="append", metavar="N",
        help="a seed to sweep with; repeat for more than one (default: 1)",
    )
    parser.add_argument(
        "--cases", type=int, default=DEFAULT_CASES, metavar="N",
        help=f"random sources per language per seed (default: {DEFAULT_CASES})",
    )
    parser.add_argument(
        "--shrink-probes", type=int, default=400, metavar="N",
        help="single-token removals the shrinker may try per signature (default: 400)",
    )
    arguments = parser.parse_args(argv)
    for executable in (RUST, OCAML):
        if not executable.exists():
            parser.error(f"{executable.relative_to(ROOT)} is not built; see the module docstring")
    seeds = arguments.seed or [1]
    languages = built_in_languages()

    found = {}
    total = 0
    diverged = 0
    for seed in seeds:
        requests, sources, divergences = sweep(seed, arguments.cases, languages)
        total += len(requests)
        diverged += len(divergences)
        for item, left, right in divergences:
            key = signature(item["language"], left.get("ok", left), right.get("ok", right))
            if key in found and len(assemble(sources[item["id"]])) >= len(found[key][0]):
                continue
            repro, shrunk_left, shrunk_right = shrink(
                item, sources[item["id"]], item["language"], arguments.shrink_probes
            )
            if key not in found or len(repro) < len(found[key][0]):
                found[key] = (repro, item, shrunk_left, shrunk_right)

    print(
        f"{total} request(s) over {len(seeds)} seed(s) x {len(languages)} language(s) "
        f"x {arguments.cases} source(s); {diverged} divergent, "
        f"{len(found)} distinct signature(s)"
    )
    for key, (repro, item, left, right) in sorted(found.items(), key=lambda entry: str(entry[0])):
        print("=" * 78)
        print(f"language={item['language']} operation={item['operation']} options={item['options']}")
        print(f"fields  ={' '.join(key[1])}")
        print(f"source  ={repro!r}")
        print(f"rust    ={json.dumps(left.get('ok', left), sort_keys=True)[:800]}")
        print(f"ocaml   ={json.dumps(right.get('ok', right), sort_keys=True)[:800]}")
    return 1 if found else 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
