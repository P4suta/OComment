#!/usr/bin/env python3
"""Hold both lockfiles to a ledger of advisories somebody decided about.

GitHub raises Dependabot alerts on this repository and they are worth having,
but they answer a different question than a gate does. An alert arrives after a
change is merged, and it can be triaged away: both `qs` advisories here had
been raised and auto-dismissed as low-impact development dependencies, so a run
asking for *open* alerts saw none while the lockfile still carried them. The
fixed version had been permitted by the declared range the whole time.

So this asks OSV, which aggregates RustSec and the GitHub Advisory Database,
about every version in `rust/Cargo.lock` and `editors/vscode/package-lock.json`,
and holds the answer to a ledger below. Two ways to fail, because a list that
only grows is one that ends up describing a repository that no longer exists:

  * an advisory nobody has written down -- somebody has to decide about it
  * a ledger entry OSV no longer reports -- the reason for it is gone, and an
    entry kept past its reason is an exemption nobody is reading

No advisory is classified automatically. OSV does not carry RustSec's
`informational` flag, and the difference between "unmaintained" and "exploitable
tomorrow" is a judgement about this project rather than a field to read. Every
entry therefore carries the argument for itself.

    python3 tools/check_advisories.py
    python3 tools/check_advisories.py --best-effort   # local use only
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys
import tomllib
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[1]
OSV = "https://api.osv.dev/v1/querybatch"

# NOTE: Keyed by advisory, with the package it is about, so a ledger entry
# NOTE: cannot quietly come to excuse a different dependency that happens to
# NOTE: draw the same advisory.
ACCEPTED: dict[str, tuple[str, str]] = {
    "RUSTSEC-2025-0141": (
        "bincode",
        "Unmaintained, not a vulnerability. Reached through `wasmtime-environ`, "
        "which this crate uses to read component-model metadata and never to "
        "execute anything. Replacing it means replacing that dependency, which "
        "is a change to how plugins are parsed rather than a patch.",
    ),
    "RUSTSEC-2025-0057": (
        "fxhash",
        "Unmaintained, not a vulnerability. Used only by the vendored "
        "`wasm_component_layer` and `wasm_runtime_layer` under "
        "`rust/ocomment/src/runtime/`, which is kept close to upstream on "
        "purpose; swapping the hasher there is a divergence with no security "
        "argument behind it.",
    ),
    "RUSTSEC-2024-0436": (
        "paste",
        "Unmaintained, not a vulnerability. A proc-macro reached through "
        "`wasmi_core`, so it runs at build time and ships nothing.",
    ),
}


class Unreadable(Exception):
    """OSV could not be asked, which is not the same as having nothing to say."""


def crates() -> list[tuple[str, str, str]]:
    """Every crate the Rust lockfile pins, as `(ecosystem, name, version)`."""
    lock = tomllib.loads((ROOT / "rust/Cargo.lock").read_text(encoding="utf-8"))
    # NOTE: `source` is absent for the workspace's own members, which have no
    # NOTE: registry to have an advisory in.
    return [
        ("crates.io", package["name"], package["version"])
        for package in lock["package"]
        if "source" in package
    ]


def npm_packages() -> list[tuple[str, str, str]]:
    """Every package the extension's lockfile pins."""
    lock = json.loads(
        (ROOT / "editors/vscode/package-lock.json").read_text(encoding="utf-8")
    )
    found = []
    for path, meta in lock.get("packages", {}).items():
        if not path or "version" not in meta:
            continue
        found.append(("npm", path.split("node_modules/")[-1], meta["version"]))
    return found


def advisories(packages: list[tuple[str, str, str]]) -> dict[str, set[str]]:
    """Advisory id to the package names it was reported against."""
    queries = [
        {"package": {"name": name, "ecosystem": ecosystem}, "version": version}
        for ecosystem, name, version in packages
    ]
    request = urllib.request.Request(
        OSV,
        data=json.dumps({"queries": queries}).encode("utf-8"),
        headers={"Content-Type": "application/json"},
    )
    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            results = json.load(response)["results"]
    except (urllib.error.URLError, TimeoutError, KeyError) as error:
        raise Unreadable(str(error)) from error
    found: dict[str, set[str]] = {}
    for query, result in zip(packages, results, strict=True):
        for vulnerability in result.get("vulns", []):
            found.setdefault(vulnerability["id"], set()).add(query[1])
    return found


def judge(found: dict[str, set[str]]) -> list[str]:
    failures = []
    for identifier, names in sorted(found.items()):
        if identifier not in ACCEPTED:
            failures.append(
                f"{identifier} is reported against {', '.join(sorted(names))} and "
                f"is not in the ledger. Fix it, or write down why it is accepted."
            )
            continue
        package, _ = ACCEPTED[identifier]
        if package not in names:
            failures.append(
                f"{identifier} is accepted here for `{package}` and OSV now "
                f"reports it against {', '.join(sorted(names))}"
            )
    for identifier, (package, _) in sorted(ACCEPTED.items()):
        if identifier not in found:
            failures.append(
                f"{identifier} ({package}) is in the ledger and OSV no longer "
                f"reports it. Remove the entry: an exemption kept past its reason "
                f"is one nobody is reading."
            )
    return failures


def self_test() -> int:
    """Both directions, on every run, with no network.

    A ledger that only fails in the direction somebody happened to test is the
    half-gate this file exists to avoid being.
    """
    unknown = judge({"GHSA-nobody-wrote-this": {"somewhere"}})
    if not any("not in the ledger" in failure for failure in unknown):
        print("self-test: an unrecorded advisory was accepted", file=sys.stderr)
        return 1
    stale = judge({})
    if len(stale) != len(ACCEPTED):
        print("self-test: a ledger entry OSV no longer reports was kept", file=sys.stderr)
        return 1
    moved = judge({next(iter(ACCEPTED)): {"a-different-crate"}})
    if not any("now reports it against" in failure for failure in moved):
        print("self-test: an entry excused a package it is not about", file=sys.stderr)
        return 1
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--best-effort",
        action="store_true",
        help="report and pass when OSV cannot be read; for a laptop, never for CI",
    )
    arguments = parser.parse_args()

    if self_test() != 0:
        return 1

    packages = crates() + npm_packages()
    try:
        found = advisories(packages)
    except Unreadable as error:
        if arguments.best_effort:
            print(f"not checked: OSV could not be read ({error})")
            return 0
        print(
            f"could not read OSV ({error}). A dependency gate that passes when it "
            f"cannot run is not a gate.",
            file=sys.stderr,
        )
        return 1

    failures = judge(found)
    if failures:
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1
    print(
        f"{len(packages)} pinned versions carry {len(found)} advisories, "
        f"every one of them written down"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
