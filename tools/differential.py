#!/usr/bin/env python3
"""Run the shared spec fixture corpus against the Rust and OCaml implementations.

Every case comes from `spec/fixtures/v1/*.json`; this file holds no fixture
bytes of its own. Each case becomes one normalized JSONL request, and the two
responses must be equal byte for byte. A case that carries an `expect` block is
additionally checked against that block, so the corpus pins absolute behaviour
rather than only agreement.

    python3 tools/differential.py             compare, and check every expectation
    python3 tools/differential.py --record    also record the missing expectations

`--record` writes an `expect` block into every case that has none, but only
once the two implementations have agreed on every case, which is what makes a
recorded block a record of the specification rather than of one implementation.
It never overwrites a block that is already there: re-recording an intentional
change means deleting that case's `expect` block first, in the same commit that
argues for the change.
"""

import base64
import json
import pathlib
import subprocess
import sys
import unicodedata

ROOT = pathlib.Path(__file__).resolve().parents[1]
RUST = ROOT / "rust/target/debug/examples/ref_driver"
OCAML = ROOT / "ocaml/_build/default/bin/main.exe"
CORPUS = ROOT / "spec/fixtures/v1"

# INVARIANT: Hazards live in `spec/`, never in this file. The floor below is
# INVARIANT: what says so out loud: deleting a case, or silently dropping a
# INVARIANT: corpus file, fails the run instead of quietly shrinking the gate.
MINIMUM_CASES = 269

DEFAULT_OPTIONS = {"policy": "safe", "layout": "lines"}
PAYLOAD_KEYS = ("spans", "edits", "profile")
FIELD_ORDER = ("id", "language", "dialect", "operation", "options", "spans", "edits", "profile",
               "source_utf8", "source_base64", "note", "expect")

# NOTE: An output longer than this is left unrecorded rather than inlined; the
# NOTE: comment spans still pin the case and the comparison still covers the
# NOTE: bytes. Only the Unicode width sweep is anywhere near it.
MAX_RECORDED_OUTPUT = 1024

# NOTE: Characters a JSON tool, an editor, or a terminal might normalise away.
# NOTE: A fixture carrying one of these is written as base64 instead.
TEXT_SAFE_CONTROL = frozenset("\n\r\t")
TEXT_UNSAFE = frozenset("\u2028\u2029")


def load_documents():
    """Every corpus document, in file-name order, with the path it was read from."""
    documents = []
    for path in sorted(CORPUS.glob("*.json")):
        document = json.loads(path.read_text(encoding="utf-8"))
        if document.get("version") != 1:
            raise SystemExit(f"{path.name}: unsupported corpus version {document.get('version')!r}")
        documents.append((path, document))
    return documents


def load_cases(documents):
    """Every case across the documents, in order, with its id unique corpus-wide."""
    cases = []
    origin = {}
    for path, document in documents:
        for case in document["cases"]:
            identifier = case["id"]
            if identifier in origin:
                raise SystemExit(
                    f"duplicate fixture id `{identifier}` in {origin[identifier]} and {path.name}"
                )
            origin[identifier] = path.name
            cases.append(case)
    if len(cases) < MINIMUM_CASES:
        raise SystemExit(
            f"the corpus holds {len(cases)} case(s), fewer than the {MINIMUM_CASES} required; "
            f"cases live in {CORPUS.relative_to(ROOT)}/*.json"
        )
    return cases


def source_bytes(case):
    """The case source as bytes, whichever of the two encodings it uses."""
    encoded = "source_base64" in case
    text = "source_utf8" in case
    if encoded == text:
        raise SystemExit(f"{case['id']}: exactly one of `source_utf8` and `source_base64`")
    if encoded:
        return base64.b64decode(case["source_base64"], validate=True)
    return case["source_utf8"].encode("utf-8")


def request(case):
    """One JSONL request for a case; the response carries the case id back."""
    options = dict(DEFAULT_OPTIONS)
    options.update(case.get("options", {}))
    if "dialect" in case:
        options["dialect"] = case["dialect"]
    value = {
        "id": case["id"],
        "operation": case.get("operation", "transform"),
        "language": case["language"],
        "source_base64": base64.b64encode(source_bytes(case)).decode(),
        "options": options,
    }
    for key in PAYLOAD_KEYS:
        if key in case:
            value[key] = case[key]
    return value


def expected_bytes(expect):
    """The output bytes an `expect` block pins, or `None` when it pins none."""
    if "output_base64" in expect:
        return base64.b64decode(expect["output_base64"], validate=True)
    if "output_utf8" in expect:
        return expect["output_utf8"].encode("utf-8")
    return None


def observed_comments(report):
    """A report's comments in the shape an `expect` block records them."""
    return [
        {
            "start": comment["span"]["start"],
            "end": comment["span"]["end"],
            "kind": comment["kind"],
            "action": comment["disposition"]["action"],
        }
        for comment in report["comments"]
    ]


def observed_diagnostics(report):
    """A report's diagnostics in the shape an `expect` block records them."""
    return [
        {"code": item["code"], "start": item["span"]["start"], "end": item["span"]["end"]}
        for item in report["diagnostics"]
    ]


def check_expect(case, payload):
    """Every way `payload` departs from the case's recorded `expect` block."""
    expect = case["expect"]
    # NOTE: `transform` nests the report; `scan` is the report.
    report = payload.get("report", payload)
    failures = []
    if "valid" in expect and report["valid"] != expect["valid"]:
        failures.append(f"valid: expected {expect['valid']}, got {report['valid']}")
    for name, observed in (
        ("comments", observed_comments),
        ("diagnostics", observed_diagnostics),
    ):
        if name in expect:
            actual = observed(report)
            if actual != expect[name]:
                failures.append(
                    f"{name}: expected {json.dumps(expect[name])}, got {json.dumps(actual)}"
                )
    wanted = expected_bytes(expect)
    if wanted is not None:
        if "output_base64" not in payload:
            failures.append("output: this operation writes no bytes to pin")
        else:
            actual = base64.b64decode(payload["output_base64"], validate=True)
            if actual != wanted:
                failures.append(f"output: expected {wanted!r}, got {actual!r}")
    return failures


def run(executable, requests):
    """Feed every request to one implementation and parse its responses."""
    payload = "".join(json.dumps(item, separators=(",", ":")) + "\n" for item in requests)
    completed = subprocess.run([str(executable)], input=payload, text=True, capture_output=True, check=True)
    return [json.loads(line) for line in completed.stdout.splitlines()]


def text_safe(raw):
    """Whether bytes are safe to carry as a JSON string rather than as base64."""
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        return False
    return all(
        character in TEXT_SAFE_CONTROL
        or (character not in TEXT_UNSAFE and unicodedata.category(character)[0] != "C")
        for character in text
    )


def recorded_expect(payload):
    """The `expect` block for one response, in the shape the corpus stores."""
    report = payload.get("report", payload)
    expect = {}
    if "comments" in report:
        expect["valid"] = report["valid"]
        expect["comments"] = observed_comments(report)
        expect["diagnostics"] = observed_diagnostics(report)
    if "output_base64" in payload:
        output = base64.b64decode(payload["output_base64"], validate=True)
        if len(output) <= MAX_RECORDED_OUTPUT:
            if text_safe(output):
                expect["output_utf8"] = output.decode("utf-8")
            else:
                expect["output_base64"] = base64.b64encode(output).decode()
    return expect


def record(documents, agreed):
    """Write an `expect` block into every case that has none. Returns how many."""
    added = 0
    for path, document in documents:
        for case in document["cases"]:
            if "expect" in case:
                continue
            case["expect"] = recorded_expect(agreed[case["id"]])
            added += 1
        document["cases"] = [
            {key: case[key] for key in FIELD_ORDER if key in case} for case in document["cases"]
        ]
        text = json.dumps(document, indent=2, ensure_ascii=False) + "\n"
        if text != path.read_text(encoding="utf-8"):
            path.write_text(text, encoding="utf-8")
            print(f"{path.name}: rewritten")
    return added


def main(argv):
    unknown = [argument for argument in argv if argument != "--record"]
    if unknown:
        print(f"unsupported argument(s): {' '.join(unknown)}", file=sys.stderr)
        return 2
    documents = load_documents()
    cases = load_cases(documents)
    requests = [request(case) for case in cases]
    rust = run(RUST, requests)
    ocaml = run(OCAML, requests)
    failures = 0
    expected = 0
    for case, left, right in zip(cases, rust, ocaml):
        label = f"{case['id']} ({case['language']})"
        if left != right:
            failures += 1
            print(f"mismatch: {label}", file=sys.stderr)
            print(json.dumps({"rust": left, "ocaml": right}, indent=2), file=sys.stderr)
            continue
        # NOTE: Both implementations refusing a case alike is still a corpus
        # NOTE: bug: every case is meant to run, and there is no way to record
        # NOTE: an expected refusal.
        if "ok" not in left:
            failures += 1
            print(f"refused: {label} {left.get('error')!r}", file=sys.stderr)
            continue
        if "expect" not in case:
            continue
        expected += 1
        for problem in check_expect(case, left["ok"]):
            failures += 1
            print(f"expect: {label} {problem}", file=sys.stderr)
    if failures:
        print(f"{failures} differential fixture failure(s)", file=sys.stderr)
        return 1
    print(f"{len(requests)} Rust/OCaml differential fixtures passed, {expected} against a recorded expectation")
    if "--record" in argv:
        agreed = {case["id"]: response["ok"] for case, response in zip(cases, rust)}
        added = record(documents, agreed)
        print(f"recorded {added} new expectation(s); {expected} were already recorded")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
