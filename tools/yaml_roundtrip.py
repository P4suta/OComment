#!/usr/bin/env python3
"""Removing a comment from a YAML document must not change what it parses to.

Every other language OComment knows lets a removal be judged by the bytes it
leaves behind. YAML does not: a block scalar decides where its body ends from
the lines *below* it, so the hole a removal leaves on a line can be read back
as content of the scalar above it. That is a property of the parsed value, not
of the bytes, and no byte-level fixture can state it. This tool states it.

    python3 tools/yaml_roundtrip.py                 the full sweep: corpus, sweeps, 2400 generated
    python3 tools/yaml_roundtrip.py --cases 200     what CI runs, to a time budget
    python3 tools/yaml_roundtrip.py --cases 20000   a longer sweep
    python3 tools/yaml_roundtrip.py --seed 7        a different generated set

The documents come from four places:

* every YAML case in `spec/fixtures/v1/*.json`, so the corpus is re-read as
  values rather than as bytes;
* a systematic sweep of every block scalar header crossed with every short
  arrangement of blank, comment, and directive lines under one, which is where
  the hazard lives;
* a second sweep of the same headers over trails whose comments sit *below* the
  body's own indentation, where a surviving comment is what the body would
  swallow and the comment above it is the only thing holding it out;
* a pseudo-random generator of nested mappings, sequences, and block scalars
  with comments in every position, in LF and in CRLF.

A document PyYAML rejects before the removal is skipped, not failed: YAML has
shapes a lexer cannot rule out and a parser will not take, and this tool is
about the ones that parse. What it asserts is that
`yaml.safe_load_all(before) == yaml.safe_load_all(after)` for every layout and
every policy, and that a document that parsed before still parses after.

The layout/policy passes are independent, so they run concurrently: the cost of
a pass is one `fsync` per rewritten file, which is latency rather than work, and
overlapping them is what keeps the CI step inside its budget.

Needs PyYAML (`pip install pyyaml`) and a built `ocomment`.
"""

import argparse
import base64
import itertools
import json
import math
import pathlib
import random
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor

try:
    import yaml
except ModuleNotFoundError:
    raise SystemExit("yaml_roundtrip.py needs PyYAML: pip install pyyaml")

ROOT = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_BINARY = ROOT / "rust/target/debug/ocomment"
CORPUS = ROOT / "spec/fixtures/v1"
LAYOUTS = ("lines", "columns", "compact")
# NOTE: `all` takes every comment out, which is the widest removal there is;
# NOTE: `safe` keeps the directives, which is the only way to reach the shapes
# NOTE: where a surviving comment still shelters the empty lines under it;
# NOTE: `legal` keeps a licence notice, which is a second kind of survivor and
# NOTE: reaches those shapes through a comment no directive marker explains.
POLICIES = ("safe", "legal", "all")

# NOTE: The sweep the docstring calls the full one, and the size the floor below
# NOTE: is written against.
DEFAULT_CASES = 2400

# INVARIANT: The generated set is the gate, so a run whose generator produced
# INVARIANT: almost nothing parseable is a broken generator rather than a pass.
# INVARIANT: This many *generated* documents have to parse before the removal;
# INVARIANT: the corpus and the sweeps are counted separately and do not fill it.
MINIMUM_CHECKED = 2000

# INVARIANT: The same floor as a fraction, for the shorter runs CI asks for. A
# INVARIANT: full-length run is still held to the absolute count above, so this
# INVARIANT: relaxes nothing there; it is what keeps a generator that started
# INVARIANT: emitting garbage from passing a `--cases 200` run.
MINIMUM_PARSE_RATE = MINIMUM_CHECKED / DEFAULT_CASES

# NOTE: How many `ocomment fix` runs are kept in flight. Each one costs an
# NOTE: `fsync` per rewritten file, so the sweep is latency-bound rather than
# NOTE: CPU-bound and oversubscribing the cores is what makes it finish.
DEFAULT_JOBS = 24


def minimum_checked(cases):
    """How many generated documents must parse for a run of `cases` to count."""
    if cases >= DEFAULT_CASES:
        return MINIMUM_CHECKED
    return math.ceil(MINIMUM_PARSE_RATE * cases)


# INVARIANT: A run where nothing was removed proves nothing at all, so the
# INVARIANT: fraction of documents the binary actually rewrote is checked too.
# INVARIANT: This is what would catch a harness that quietly stopped stripping.
MINIMUM_REWRITTEN = 0.5

BLOCK_HEADERS = ("|", "|-", "|+", ">", ">-", ">+", "|2", "|2+", "|+2", ">2-")

# NOTE: A comment `safe` keeps. Under that policy it still shelters the empty
# NOTE: lines beneath it, so the removals around it must leave its own line
# NOTE: alone and still take what they were sheltering.
DIRECTIVE = "# yamllint disable-line rule:line-length"

# NOTE: A second directive marker, so a kept trail line is not always the same
# NOTE: bytes, and a licence notice, which only `legal` keeps -- between them
# NOTE: every policy under test has a comment it will not remove.
SCHEMA = "# yaml-language-server: $schema=https://example.test/schema.json"
LICENSE = "# SPDX-License-Identifier: MIT"

# NOTE: The comments a trail may hold, by the policy that keeps each one.
SURVIVORS = (DIRECTIVE, SCHEMA, LICENSE)


def corpus_documents():
    """Every YAML source in the shared fixture corpus, by fixture id."""
    documents = []
    for path in sorted(CORPUS.glob("*.json")):
        for case in json.loads(path.read_text(encoding="utf-8"))["cases"]:
            if case.get("language") != "yaml":
                continue
            if "source_utf8" in case:
                source = case["source_utf8"]
            else:
                try:
                    source = base64.b64decode(case["source_base64"]).decode("utf-8")
                except UnicodeDecodeError:
                    continue
            documents.append((f"corpus-{case['id']}", source))
    return documents


def swept_documents():
    """The block scalar hazard, enumerated rather than sampled.

    One header, one body, then every arrangement of up to four blank and
    comment lines under it, with and without a blank line between the body and
    the first of them, and with and without a sibling key below. The two
    reviewer-verified shapes are members of this sweep, so it fails loudly
    while either is unfixed instead of waiting for the generator to stumble
    onto them.
    """
    documents = []
    patterns = [""]
    for width in range(1, 5):
        patterns.extend("".join(item) for item in itertools.product("BC", repeat=width))
    for width in range(1, 4):
        patterns.extend(
            "".join(item)
            for item in itertools.product("BCD", repeat=width)
            if "D" in item
        )
    for header in BLOCK_HEADERS:
        for pattern in patterns:
            for lead in ("", "B"):
                for tail in ("z: 1", ""):
                    lines = [f"k: {header}", "  a", "  b"]
                    for index, item in enumerate(lead + pattern):
                        if item == "B":
                            lines.append("")
                        elif item == "D":
                            lines.append(DIRECTIVE)
                        else:
                            lines.append(f"# c{index}")
                    if tail:
                        lines.append(tail)
                    identifier = (
                        f"sweep-{header}-{lead or 'x'}-{pattern or 'none'}-{tail or 'eof'}"
                    )
                    documents.append((identifier, "\n".join(lines) + "\n"))
    return documents


def structural_documents():
    """The trail read against the body's *content* indentation, enumerated.

    `swept_documents` writes every trail line in column zero, which is always
    shallower than the body and therefore always safe to take. The hazard this
    sweep is about needs the other side: a trail line indented to the content
    depth or past it, which the body swallows the moment the line above it goes
    away. The body is written at two depths and the headers include explicit
    indentation indicators, so the content depth and the floor a body line has
    to clear are different numbers in some of these and equal in others.

    Each trail line is one of: a blank; an ordinary comment in column zero or at
    the body depth; a survivor -- a directive or a licence notice, kept by one of
    the policies -- in either place. Two lines are enough to state the hazard --
    one that ends the body and one the body would take back -- so the product
    stops there and the generator below reaches the longer trails.
    """
    documents = []
    survivors = {"D": DIRECTIVE, "S": SCHEMA, "L": LICENSE}
    alphabet = ("B", "C", "c", "D", "d", "S", "s", "L", "l")
    patterns = []
    for width in (1, 2):
        patterns.extend(
            "".join(item)
            for item in itertools.product(alphabet, repeat=width)
            # NOTE: A trail with no indented line in it is what the sweep above
            # NOTE: already enumerates, in more arrangements than this one.
            if any(item.islower() for item in item)
        )
    for header in BLOCK_HEADERS:
        for depth in (2, 3):
            for pattern in patterns:
                lines = [f"k: {header}", " " * depth + "a"]
                for index, item in enumerate(pattern):
                    if item == "B":
                        lines.append("")
                        continue
                    indent = " " * depth if item.islower() else ""
                    body = survivors.get(item.upper(), f"# c{index}")
                    lines.append(indent + body)
                lines.append("z: 1")
                documents.append(
                    (f"structural-{header}-{depth}-{pattern}", "\n".join(lines) + "\n")
                )
    return documents


class Generator:
    """Nested YAML with comments wherever one may stand."""

    def __init__(self, rng):
        self.rng = rng
        self.counter = 0

    def name(self, prefix):
        self.counter += 1
        return f"{prefix}{self.counter}"

    def comment_line(self, limit):
        """A whole-line comment indented anywhere it is still a comment.

        One in three is a comment some policy under test keeps, because a trail
        the removal empties out states only half the hazard: the other half
        needs a survivor left standing in it.
        """
        indent = self.rng.randrange(0, max(limit, 0) + 1)
        if self.rng.random() < 0.3:
            return " " * indent + self.rng.choice(SURVIVORS)
        return " " * indent + f"# {self.name('note')}"

    def separators(self, lines, limit):
        """Blank and comment lines between two siblings, in any order."""
        for _ in range(self.rng.randrange(0, 3)):
            if self.rng.random() < 0.45:
                lines.append("")
            else:
                lines.append(self.comment_line(limit))

    def scalar(self):
        return self.rng.choice(
            [
                "1",
                "true",
                "null",
                "plain text",
                '"quoted # hash"',
                "'single # hash'",
                "[1, 2]",
                "{a: 1}",
                "0.5",
            ]
        )

    def header(self, extra):
        """A block scalar header, in every spelling one may take."""
        style = self.rng.choice("|>")
        chomping = self.rng.choice(["", "", "-", "+", "+"])
        indicator = str(extra) if self.rng.random() < 0.3 else ""
        if indicator and chomping and self.rng.random() < 0.5:
            return style + chomping + indicator
        return style + indicator + chomping

    def block_scalar(self, lines, head, owner_indent, own_line):
        """A block scalar value plus its body, its trail, and the trail's comments.

        The indentation indicator counts from the node the scalar hangs off, so
        the body is written at `owner_indent + extra` whether or not the
        indicator spells `extra` out — and `own_line` puts the header on the
        line below its key, where its own column says nothing about how deep
        the body has to be. The trail below the body is the hazard: a blank
        line there is content under `+` and separation under a comment, which
        is the whole of what a removal has to get right.

        The trail is swept on both sides of the body's own indentation. A trail
        comment shallower than the content is what ends the scalar and is safe
        to take; one as deep as the content is content again the moment the
        line above it goes, and capping the indent at the body's depth would
        have left that half of the hazard ungenerated.
        """
        extra = self.rng.choice([1, 2, 2, 3])
        header = self.header(extra)
        comment = f" # {self.name('head')}" if self.rng.random() < 0.2 else ""
        if own_line:
            lines.append(head.rstrip())
            lines.append(" " * (owner_indent + 2) + header + comment)
        else:
            lines.append(head + header + comment)
        body = " " * (owner_indent + extra)
        for _ in range(self.rng.randrange(1, 4)):
            lines.append(body + self.name("line"))
            if self.rng.random() < 0.25:
                lines.append("")
        for _ in range(self.rng.randrange(0, 4)):
            if self.rng.random() < 0.5:
                lines.append("")
            else:
                lines.append(self.comment_line(owner_indent + extra + 2))

    def value(self, lines, indent, depth, prefix):
        """One `key:`/`- ` entry, whose value may be inline, nested, or a block."""
        roll = self.rng.random()
        if depth <= 0 or roll < 0.4:
            trailing = f" # {self.name('eol')}" if self.rng.random() < 0.3 else ""
            lines.append(" " * indent + prefix + self.scalar() + trailing)
        elif roll < 0.65:
            lines.append(" " * indent + prefix.rstrip())
            self.separators(lines, indent + 2)
            self.node(lines, indent + 2, depth - 1)
        else:
            self.block_scalar(
                lines, " " * indent + prefix, indent, self.rng.random() < 0.25
            )

    def node(self, lines, indent, depth):
        if self.rng.random() < 0.55:
            for index in range(self.rng.randrange(1, 4)):
                if index:
                    self.separators(lines, indent)
                self.value(lines, indent, depth, f"{self.name('k')}: ")
        else:
            for index in range(self.rng.randrange(1, 4)):
                if index:
                    self.separators(lines, indent)
                self.value(lines, indent, depth, "- ")

    def document(self):
        lines = []
        for index in range(self.rng.randrange(1, 3)):
            if index:
                lines.append("---")
            self.separators(lines, 0)
            self.node(lines, 0, self.rng.randrange(1, 4))
            self.separators(lines, 0)
        text = "\n".join(lines) + "\n"
        if self.rng.random() < 0.25:
            text = text.replace("\n", "\r\n")
        return text


def generated_documents(count, seed):
    rng = random.Random(seed)
    generator = Generator(rng)
    return [(f"gen-{index:05d}", generator.document()) for index in range(count)]


def parse(text):
    """The documents `text` holds, or `None` when PyYAML will not have it."""
    # NOTE: Every complaint a YAML parser can make means the same thing here --
    # NOTE: this document is not one the invariant is about -- so they are all
    # NOTE: caught together rather than enumerated.
    try:
        return list(yaml.safe_load_all(text))
    except Exception:
        return None


def strip_chunk(binary, layout, policy, sources, room):
    """One `ocomment fix` run over one chunk of documents, in the same order.

    One run for a whole chunk rather than one process per document: the point is
    to sweep thousands of layouts of the same hazard, and the walk is what the
    binary is for.
    """
    if room.exists():
        shutil.rmtree(room)
    room.mkdir(parents=True)
    config = room / "ocomment.toml"
    config.write_text("version = 1\n", encoding="utf-8")
    paths = []
    for index, source in enumerate(sources):
        path = room / f"doc{index:06d}.yaml"
        path.write_bytes(source.encode("utf-8"))
        paths.append(path)
    result = subprocess.run(
        [
            str(binary),
            "fix",
            "--language",
            "yaml",
            "--policy",
            policy,
            "--layout",
            layout,
            "--force-protected",
            "--config",
            str(config),
            str(room),
        ],
        capture_output=True,
        check=False,
    )
    report = result.stdout.decode("utf-8", "replace")
    # NOTE: Only documents PyYAML accepted are written here, so a file the
    # NOTE: scanner calls invalid — exit code 2 — is a disagreement worth the
    # NOTE: run, not a document to skip past.
    if result.returncode not in (0, 1) or "invalid syntax" in report:
        raise SystemExit(
            f"ocomment fix --policy {policy} --layout {layout} exited "
            f"{result.returncode}\n{report}{result.stderr.decode('utf-8', 'replace')}"
        )
    return [path.read_bytes().decode("utf-8", "replace") for path in paths]


def strip_all(binary, runs, sources, workdir, jobs):
    """Every source under every layout and policy, keyed by the pair.

    A pass costs one `fsync` per rewritten file, which is latency and not work,
    so the passes are cut into chunks and the chunks are run together: the
    binary is already parallel *within* a run, and what is left to overlap is
    the waiting. `jobs` is how many of those runs may be in flight at once.
    """
    chunks = max(1, -(-jobs // len(runs)))
    size = max(1, -(-len(sources) // chunks))
    tasks = [
        (layout, policy, start, sources[start : start + size])
        for layout, policy in runs
        for start in range(0, max(len(sources), 1), size)
    ]

    def run(task):
        layout, policy, start, chunk = task
        room = workdir / f"{layout}-{policy}-{start:07d}"
        return (layout, policy), start, strip_chunk(binary, layout, policy, chunk, room)

    outputs = {pair: [] for pair in runs}
    with ThreadPoolExecutor(max_workers=max(jobs, 1)) as pool:
        for pair, start, chunk in pool.map(run, tasks):
            outputs[pair].append((start, chunk))
    return {
        pair: [output for _, chunk in sorted(pieces) for output in chunk]
        for pair, pieces in outputs.items()
    }


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--binary", type=pathlib.Path, default=DEFAULT_BINARY)
    parser.add_argument("--cases", type=int, default=DEFAULT_CASES)
    parser.add_argument("--seed", type=int, default=11)
    parser.add_argument(
        "--show", type=int, default=10, help="how many failures to print in full"
    )
    parser.add_argument(
        "--jobs",
        type=int,
        default=DEFAULT_JOBS,
        help="how many `ocomment fix` runs may be in flight at once",
    )
    arguments = parser.parse_args(argv)
    if not arguments.binary.exists():
        raise SystemExit(
            f"{arguments.binary} is not built: "
            "cargo build --manifest-path rust/Cargo.toml -p ocomment"
        )

    generated = generated_documents(arguments.cases, arguments.seed)
    documents = (
        corpus_documents() + swept_documents() + structural_documents() + generated
    )
    generated_ids = {identifier for identifier, _ in generated}
    live = [
        (identifier, source, parsed)
        for identifier, source in documents
        for parsed in [parse(source)]
        if parsed is not None
    ]
    skipped = len(documents) - len(live)
    sources = [source for _, source, _ in live]
    failures = []
    rewritten = 0
    runs = [(layout, policy) for layout in LAYOUTS for policy in POLICIES]
    with tempfile.TemporaryDirectory(prefix="ocomment-yaml-roundtrip-") as room:
        stripped = strip_all(
            arguments.binary, runs, sources, pathlib.Path(room), arguments.jobs
        )
    for layout, policy in runs:
        for (identifier, source, parsed), output in zip(live, stripped[layout, policy]):
            rewritten += output != source
            after = parse(output)
            if after != parsed:
                failures.append(
                    (layout, policy, identifier, source, output, parsed, after)
                )

    pairs = len(live) * len(runs)
    for layout, policy, identifier, source, output, parsed, after in failures[
        : arguments.show
    ]:
        print(f"--- {identifier} [{policy}/{layout}]")
        print(f"    before {source!r}")
        print(f"     after {output!r}")
        print(f"    parsed {parsed!r}")
        print(f"        -> {after!r}" if after is not None else "        -> did not parse")
    if failures:
        tally = {}
        for failure in failures:
            tally[failure[1], failure[0]] = tally.get((failure[1], failure[0]), 0) + 1
        breakdown = ", ".join(
            f"{count} under {policy}/{layout}"
            for (policy, layout), count in sorted(tally.items())
        )
        print(
            f"{len(failures)} YAML round-trip failure(s) over {pairs} parsed "
            f"document/layout/policy triples ({breakdown}; {skipped} of "
            f"{len(documents)} documents PyYAML rejected before)"
        )
        return 1
    generated_live = sum(1 for identifier, _, _ in live if identifier in generated_ids)
    floor = minimum_checked(arguments.cases)
    if generated_live < floor:
        print(
            f"only {generated_live} generated document(s) parsed, fewer than the "
            f"{floor} this gate requires; raise --cases or fix the generator"
        )
        return 1
    if rewritten < MINIMUM_REWRITTEN * pairs:
        print(
            f"only {rewritten} of {pairs} runs changed the document at all, under the "
            f"{MINIMUM_REWRITTEN:.0%} this gate requires: the sweep is not exercising a removal"
        )
        return 1
    print(
        f"YAML round-trip holds: {pairs} parsed document/layout/policy triples "
        f"({len(live)} documents, {generated_live} of them generated, over "
        f"{len(LAYOUTS)} layouts and {len(POLICIES)} policies; {rewritten} of the "
        f"triples were actually rewritten, and {skipped} of {len(documents)} "
        f"documents were rejected by PyYAML before the removal)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
