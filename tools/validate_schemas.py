#!/usr/bin/env python3
"""Validate canonical schemas, the default config, and actual CLI JSON output."""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import tempfile
import tomllib


ROOT = pathlib.Path(__file__).resolve().parents[1]

SARIF_SCHEMA = "https://json.schemastore.org/sarif-2.1.0.json"
SARIF_LEVELS = frozenset({"none", "note", "warning", "error"})
# NOTE: What a repository-relative artifact URI is measured from, and the
# NOTE: pseudo-path a run over standard input reports, measured from nothing.
SARIF_SRCROOT = "%SRCROOT%"
SARIF_STDIN_URI = "<stdin>"


# INVARIANT: Every artifact URI the CLI emits, and the base id each one is owed.
# INVARIANT: The two halves of this rule live apart -- `output::artifact_location`
# INVARIANT: writes them and this validates them -- so the cases are pinned here
# INVARIANT: and checked by `--self-test`: a validator that disagrees with the
# INVARIANT: emitter fails a document that is perfectly good, or passes one no
# INVARIANT: code-scanning UI can resolve.
SARIF_URI_CASES: tuple[tuple[str, str | None, bool], ...] = (
    # NOTE: A path inside the checkout is resolved against the source root.
    ("a.rs", SARIF_SRCROOT, True),
    ("sub/doc.rs", SARIF_SRCROOT, True),
    # NOTE: A path that names no place inside it is under no base at all.
    (SARIF_STDIN_URI, None, True),
    ("/tmp/a.rs", None, True),
    ("C:/src/a.rs", None, True),
    ("../sibling/a.rs", None, True),
    ("sub/../../sibling/a.rs", None, True),
    # NOTE: A relative URI with no base resolves against nothing, and a base
    # NOTE: on a URI that has left the checkout claims a place it is not in.
    ("a.rs", None, False),
    ("../sibling/a.rs", SARIF_SRCROOT, False),
    ("sub/../../sibling/a.rs", SARIF_SRCROOT, False),
    ("/tmp/a.rs", SARIF_SRCROOT, False),
    ("C:/src/a.rs", SARIF_SRCROOT, False),
    (SARIF_STDIN_URI, SARIF_SRCROOT, False),
    # NOTE: Neither a `.` segment nor a backslash names a file a checkout has.
    ("./a.rs", SARIF_SRCROOT, False),
    ("sub/./doc.rs", SARIF_SRCROOT, False),
    ("sub\\doc.rs", SARIF_SRCROOT, False),
    # NOTE: A first segment of one letter and a colon is read as a drive letter
    # NOTE: wherever it turns up, and a POSIX checkout may hold a directory
    # NOTE: named `c:`. A Linux run over `c:/a.rs` really does emit
    # NOTE: `{"uri": "c:/a.rs", "uriBaseId": "%SRCROOT%"}`, which this rule then
    # NOTE: turns down; the pair below pins the limitation rather than leaving
    # NOTE: it to be met for the first time on a failing CI job.
    ("c:/a.rs", SARIF_SRCROOT, False),
    ("c:/a.rs", None, True),
)


def self_test() -> int:
    """Check the artifact-URI rule against the cases the CLI actually emits."""
    broken: list[str] = []
    for uri, base, valid in SARIF_URI_CASES:
        location: dict[str, object] = {"uri": uri}
        if base is not None:
            location["uriBaseId"] = base
        failures: list[str] = []
        check_sarif_uri(location, "self-test", failures)
        if valid and failures:
            broken.append(f"{location!r} is emitted by the CLI but rejected: {failures}")
        elif not valid and not failures:
            broken.append(f"{location!r} is accepted, but no reader can resolve it")
    if broken:
        print("tools/validate_schemas.py --self-test:")
        print("\n".join(f"  {failure}" for failure in broken))
        return 1
    return 0


def check_sarif_region(region: object, where: str, failures: list[str]) -> None:
    """SARIF text regions are 1-based and must not run backwards."""
    if not isinstance(region, dict):
        failures.append(f"{where} is not an object")
        return
    for field in ("startLine", "startColumn", "endLine", "endColumn"):
        value = region.get(field)
        if not isinstance(value, int) or isinstance(value, bool) or value < 1:
            failures.append(f"{where}.{field} is not a 1-based integer: {value!r}")
    start_line, end_line = region.get("startLine"), region.get("endLine")
    if isinstance(start_line, int) and isinstance(end_line, int) and end_line < start_line:
        failures.append(f"{where} ends on line {end_line} before line {start_line}")


def sarif_uri_is_absolute(uri: str) -> bool:
    """Whether a URI already says where it starts, so no base id may.

    A POSIX path starts at the root and a Windows one starts at a drive
    letter; the emitter turns the separators of the second around but leaves
    the drive where it found it, so `C:/src/a.rs` is what arrives here.

    The drive is recognised by shape rather than by platform: any first segment
    of a single letter and a colon counts, which a POSIX checkout is free to
    spell as an ordinary directory name. A file at `c:/a.rs` on Linux is
    emitted under `%SRCROOT%` -- correctly, since that resolves -- and read
    here as a path that already says where it starts, so the two disagree
    about it. `SARIF_URI_CASES` pins that one case.
    """
    head = uri.split("/", 1)[0]
    return uri.startswith("/") or (len(head) == 2 and head[1] == ":" and head[0].isalpha())


def check_sarif_uri(location: object, where: str, failures: list[str]) -> None:
    if not isinstance(location, dict):
        failures.append(f"{where} is not an object")
        return
    uri = location.get("uri")
    base = location.get("uriBaseId")
    if not isinstance(uri, str) or not uri:
        failures.append(f"{where}.uri is not a non-empty string: {uri!r}")
        return
    # NOTE: A path is what the emitter reports; a URL with a scheme in front
    # NOTE: of it is something else entirely, and no checkout holds one.
    if "://" in uri:
        failures.append(f"{where}.uri is a URL rather than a path: {uri!r}")
        return
    # NOTE: A code-scanning UI matches the URI against the paths the checkout
    # NOTE: uses, and neither a backslash nor a `.` segment names a file any
    # NOTE: checkout has.
    if "\\" in uri:
        failures.append(f"{where}.uri uses a backslash separator: {uri!r}")
    if uri.startswith("./") or "/./" in uri:
        failures.append(f"{where}.uri keeps a `.` segment: {uri!r}")
    # INVARIANT: A relative URI resolves against a base id. Standard input is
    # INVARIANT: not a file, an absolute path already says where it starts, and
    # INVARIANT: a path that climbs out through `..` has left the tree the base
    # INVARIANT: id would measure it from -- the emitter leaves the base off all
    # INVARIANT: three, so demanding one here would fail a document that is
    # INVARIANT: right. The climb is looked for as a path segment rather than a
    # INVARIANT: prefix: `sub/../../sibling/a.rs` leaves the tree just as surely
    # INVARIANT: as `../sibling/a.rs`, and `..pending/a.rs` does not leave it.
    if uri == SARIF_STDIN_URI or sarif_uri_is_absolute(uri) or ".." in uri.split("/"):
        if base is not None:
            failures.append(f"{where}.uriBaseId is {base!r}, but {uri!r} is under no base")
    elif base != SARIF_SRCROOT:
        failures.append(f"{where}.uriBaseId is {base!r}, expected {SARIF_SRCROOT!r}")


def check_sarif_rules(driver: dict, where: str, failures: list[str]) -> dict[str, int]:
    """The rule catalogue, as the id-to-index map a result is checked against.

    A code-scanning UI titles a finding, describes it, and links out of it
    through the rule the result names, so a result whose rule is missing --
    or whose `ruleIndex` points at somebody else's -- arrives with nothing but
    its id.
    """
    rules = driver.get("rules")
    if not isinstance(rules, list) or not rules:
        failures.append(f"{where}.rules is not a non-empty array")
        return {}
    indices: dict[str, int] = {}
    for index, rule in enumerate(rules):
        spot = f"{where}.rules[{index}]"
        if not isinstance(rule, dict):
            failures.append(f"{spot} is not an object")
            continue
        identifier = rule.get("id")
        if not isinstance(identifier, str) or not identifier:
            failures.append(f"{spot}.id is not a non-empty string: {identifier!r}")
            continue
        if identifier in indices:
            failures.append(f"{spot}.id describes `{identifier}` a second time")
        else:
            indices[identifier] = index
        for field in ("shortDescription", "fullDescription"):
            described = rule.get(field)
            text = described.get("text") if isinstance(described, dict) else None
            if not isinstance(text, str) or not text:
                failures.append(f"{spot}.{field}.text is not a non-empty string: {text!r}")
        help_uri = rule.get("helpUri")
        if not isinstance(help_uri, str) or not help_uri.startswith("https://"):
            failures.append(f"{spot}.helpUri is not an https URL: {help_uri!r}")
        configuration = rule.get("defaultConfiguration")
        level = configuration.get("level") if isinstance(configuration, dict) else None
        if level not in SARIF_LEVELS:
            failures.append(f"{spot}.defaultConfiguration.level is {level!r}, not a SARIF level")
    return indices


def check_sarif(path: pathlib.Path) -> int:
    """Structurally validate an `ocomment --format sarif` document.

    The published SARIF schema is only reachable over the network -- and the
    job that runs this on three operating systems installs no Python packages
    -- so this checks the shape and the invariants OComment guarantees
    instead: the driver names itself and its version, every rule a result
    names is described at the index the result points at, and every reported
    path is spelled the way the checkout spells it.
    """
    failures: list[str] = []
    try:
        document = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        print(f"{path}: {error}")
        return 1

    if not isinstance(document, dict):
        print(f"{path}: top level is not an object")
        return 1
    if document.get("version") != "2.1.0":
        failures.append(f"version is {document.get('version')!r}, expected '2.1.0'")
    if document.get("$schema") != SARIF_SCHEMA:
        failures.append(f"$schema is {document.get('$schema')!r}, expected {SARIF_SCHEMA!r}")

    runs = document.get("runs")
    if not isinstance(runs, list) or not runs:
        print(f"{path}: runs is not a non-empty array")
        return 1

    results_seen = 0
    rules_seen = 0
    for index, run in enumerate(runs):
        where = f"runs[{index}]"
        if not isinstance(run, dict):
            failures.append(f"{where} is not an object")
            continue
        driver = run.get("tool", {}).get("driver", {}) if isinstance(run.get("tool"), dict) else {}
        rules: dict[str, int] = {}
        if not isinstance(driver, dict) or driver.get("name") != "ocomment":
            failures.append(f"{where}.tool.driver.name is not 'ocomment'")
        else:
            if not isinstance(driver.get("informationUri"), str):
                failures.append(f"{where}.tool.driver.informationUri is missing")
            version = driver.get("version")
            if not isinstance(version, str) or not version:
                failures.append(f"{where}.tool.driver.version is not a non-empty string: {version!r}")
            rules = check_sarif_rules(driver, f"{where}.tool.driver", failures)
            rules_seen += len(rules)
        results = run.get("results")
        if not isinstance(results, list):
            failures.append(f"{where}.results is not an array")
            continue
        for position, result in enumerate(results):
            results_seen += 1
            spot = f"{where}.results[{position}]"
            if not isinstance(result, dict):
                failures.append(f"{spot} is not an object")
                continue
            rule_id = result.get("ruleId")
            if not isinstance(rule_id, str) or not rule_id:
                failures.append(f"{spot}.ruleId is not a non-empty string")
            else:
                rule_index = result.get("ruleIndex")
                if not isinstance(rule_index, int) or isinstance(rule_index, bool):
                    failures.append(f"{spot}.ruleIndex is not an integer: {rule_index!r}")
                elif rule_id not in rules:
                    failures.append(f"{spot}.ruleId `{rule_id}` is described by no rule")
                elif rules[rule_id] != rule_index:
                    failures.append(
                        f"{spot}.ruleIndex is {rule_index}, "
                        f"but `{rule_id}` is rules[{rules[rule_id]}]"
                    )
            message = result.get("message")
            if not isinstance(message, dict) or not isinstance(message.get("text"), str) or not message["text"]:
                failures.append(f"{spot}.message.text is not a non-empty string")
            if result.get("level") not in SARIF_LEVELS:
                failures.append(f"{spot}.level is {result.get('level')!r}, not a SARIF level")
            locations = result.get("locations")
            if not isinstance(locations, list) or not locations:
                failures.append(f"{spot}.locations is not a non-empty array")
                continue
            for slot, location in enumerate(locations):
                place = f"{spot}.locations[{slot}].physicalLocation"
                physical = location.get("physicalLocation") if isinstance(location, dict) else None
                if not isinstance(physical, dict):
                    failures.append(f"{place} is not an object")
                    continue
                check_sarif_uri(physical.get("artifactLocation"), f"{place}.artifactLocation", failures)
                if "region" in physical:
                    check_sarif_region(physical["region"], f"{place}.region", failures)
            for slot, fix in enumerate(result.get("fixes", []) or []):
                place = f"{spot}.fixes[{slot}]"
                changes = fix.get("artifactChanges") if isinstance(fix, dict) else None
                if not isinstance(changes, list) or not changes:
                    failures.append(f"{place}.artifactChanges is not a non-empty array")
                    continue
                for offset, change in enumerate(changes):
                    corner = f"{place}.artifactChanges[{offset}]"
                    if not isinstance(change, dict):
                        failures.append(f"{corner} is not an object")
                        continue
                    check_sarif_uri(change.get("artifactLocation"), f"{corner}.artifactLocation", failures)
                    replacements = change.get("replacements")
                    if not isinstance(replacements, list) or not replacements:
                        failures.append(f"{corner}.replacements is not a non-empty array")
                        continue
                    for edge, replacement in enumerate(replacements):
                        if not isinstance(replacement, dict):
                            failures.append(f"{corner}.replacements[{edge}] is not an object")
                            continue
                        check_sarif_region(
                            replacement.get("deletedRegion"),
                            f"{corner}.replacements[{edge}].deletedRegion",
                            failures,
                        )

    if failures:
        print(f"{path}:")
        print("\n".join(f"  {failure}" for failure in failures))
        return 1
    print(f"{path}: valid SARIF 2.1.0 with {results_seen} ocomment results under {rules_seen} rules")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--binary",
        type=pathlib.Path,
        default=ROOT / "rust/target/debug/ocomment",
    )
    parser.add_argument(
        "--sarif",
        type=pathlib.Path,
        help="validate this `--format sarif` document instead of the canonical schemas",
    )
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="check the artifact-URI rule against the URIs the CLI emits, and stop",
    )
    args = parser.parse_args()
    # INVARIANT: The rule is checked wherever this script runs: on the schema
    # INVARIANT: job that takes no arguments, and on the three-operating-system
    # INVARIANT: job that hands it a SARIF document. Neither has to remember to
    # INVARIANT: ask for it.
    if self_test() != 0:
        return 1
    if args.self_test:
        print(f"{len(SARIF_URI_CASES)} artifact URI cases agree with the emitter")
        return 0
    if args.sarif is not None:
        return check_sarif(args.sarif)

    import jsonschema

    config_schema = json.loads((ROOT / "spec/config.schema.json").read_text())
    result_schema = json.loads((ROOT / "spec/result.schema.json").read_text())
    jsonschema.Draft202012Validator.check_schema(config_schema)
    jsonschema.Draft202012Validator.check_schema(result_schema)

    with (ROOT / "spec/default-config.toml").open("rb") as stream:
        jsonschema.validate(tomllib.load(stream), config_schema)

    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"CLI binary does not exist: {binary}")
    with tempfile.TemporaryDirectory(prefix="ocomment-schema-") as raw:
        fixture = pathlib.Path(raw) / "schema.rs"
        fixture.write_bytes(b"let value = 1; // removable\n")
        completed = subprocess.run(
            [str(binary), "scan", str(fixture), "--format", "json"],
            check=True,
            capture_output=True,
        )
    jsonschema.validate(json.loads(completed.stdout), result_schema)
    print("config and result schemas validate canonical runtime examples")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
