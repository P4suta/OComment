#!/usr/bin/env python3
"""Ask the upstream repositories whether the reviewed action pins are true.

`check_ci_contracts.py` holds every third-party action to a reviewed table:
SHA-pinned, in the table, the same SHA, the same version comment. All of that
is decided inside this repository, and it is airtight about everything a file
here can be wrong about.

It cannot be wrong about the one thing that matters most. The table says a SHA
*is* a version, and whether that is true lives in someone else's repository. A
mistyped digest that happens to be a real commit, a bump whose label does not
match the commit it carries, a tag an upstream force-moved after review -- each
of those leaves this repository perfectly self-consistent and running code
nobody looked at.

So this asks. Every entry labelled with a version has to be the commit that
version's tag resolves to, today, according to the repository that publishes
it. An action that ships no version tags at all is a different thing and is
named here with the reason, because a check that silently accepts whatever it
cannot verify is not a check.

    python3 tools/check_action_pins.py
    python3 tools/check_action_pins.py --best-effort   # local use only
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import urllib.error
import urllib.request

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from check_ci_contracts import PINS  # noqa: E402

API = "https://api.github.com"


class Refused(Exception):
    """The API answered, and the answer was not the one asked for."""

# NOTE: `vN.N` and `vN.N.N` both count.
# NOTE: What does not count is a moving major like `v3`, which names whatever its publisher last pointed it at rather than the commit under review -- so a table entry carrying one cannot be checked and must not be allowed to look as though it was.
EXACT_VERSION = re.compile(r"^v\d+(?:\.\d+)+$")

# NOTE: Actions that publish no version tags, with what is true of them instead.
# NOTE: `dtolnay/rust-toolchain` force-updates a `stable` branch as Rust releases, so the commit under review is reachable from nothing today -- which is the argument for pinning it by digest rather than against it, and the reason the only thing worth asserting is that the digest names a commit of that repository.
UNTAGGED = {
    "dtolnay/rust-toolchain": "publishes no version tags; `stable` is a branch it force-updates",
}


def github_token() -> str | None:
    """A token to read with, from the environment or from a signed-in `gh`.

    Unauthenticated reads are sixty an hour for the whole machine, which twenty
    pins exhaust in three runs; a developer would then watch `preflight` fail
    for a reason that has nothing to do with their change. CI already sets
    `GITHUB_TOKEN`, and a laptop with this repository checked out almost always
    has `gh` signed in. Neither grants anything this does not already have --
    the repositories being read are public.
    """
    for name in ("GITHUB_TOKEN", "GH_TOKEN"):
        if os.environ.get(name):
            return os.environ[name]
    if shutil.which("gh") is None:
        return None
    result = subprocess.run(
        ["gh", "auth", "token"], capture_output=True, text=True, check=False
    )
    return result.stdout.strip() or None if result.returncode == 0 else None


def fetch(path: str) -> dict | None:
    """One GitHub API read, or `None` when the object is not there."""
    request = urllib.request.Request(f"{API}{path}")
    request.add_header("Accept", "application/vnd.github+json")
    token = github_token()
    if token:
        request.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        if error.code == 404:
            return None
        # NOTE: A refusal is not an absence.
        # NOTE: Being rate-limited or told no means the answer exists and was not read, which `--skip-when-offline` must not be allowed to turn into a pass -- that flag is for a laptop with no network, and a gate that treats "would not say" as "nothing to say" is the failure this whole file is about.
        raise Refused(f"{error.code} {error.reason}") from error


def commit_for_tag(repo: str, tag: str) -> str | None:
    """The commit `tag` names in `repo`, following an annotated tag object."""
    reference = fetch(f"/repos/{repo}/git/ref/tags/{tag}")
    if reference is None:
        return None
    target = reference["object"]
    if target["type"] != "tag":
        return target["sha"]
    annotated = fetch(f"/repos/{repo}/git/tags/{target['sha']}")
    return annotated["object"]["sha"] if annotated else target["sha"]


def repository_of(action: str) -> str:
    """`github/codeql-action/init` is published by `github/codeql-action`."""
    return "/".join(action.split("/")[:2])


def check(name: str, digest: str, label: str) -> list[str]:
    repo = repository_of(name)
    if not EXACT_VERSION.match(label):
        if name not in UNTAGGED:
            return [
                f"{name}: `{label}` is not a version, and the table says nothing "
                f"about why. A moving tag names whatever its publisher last "
                f"pointed it at, not the commit that was reviewed."
            ]
        if fetch(f"/repos/{repo}/commits/{digest}") is None:
            return [f"{name}: {digest} is not a commit of {repo}"]
        return []
    if name in UNTAGGED:
        return [f"{name}: carries version {label} and is also listed as untagged"]
    tagged = commit_for_tag(repo, label)
    if tagged is None:
        return [f"{name}: {repo} publishes no tag {label}"]
    if tagged != digest:
        # NOTE: Whole digests.
        # NOTE: Abbreviating them printed the same twelve characters twice under the word "but", because the character that differed was past the cut -- a mismatch reported as two identical strings, which reads as a bug in the checker rather than a finding about the pin.
        return [
            f"{name}: the table says {label} is\n"
            f"  {digest}\n"
            f"but {repo} says {label} is\n"
            f"  {tagged}"
        ]
    return []


def self_test() -> int:
    """A label that cannot be checked has to be refused, offline and always.

    The rest of this file needs the network and a negative control that only
    runs when the network is there is one that stops running on the day it is
    needed most.
    """
    cases = [
        ("someone/action", "v3", ["is not a version"]),
        ("someone/action", "latest", ["is not a version"]),
        ("someone/action", "stable toolchain action", ["is not a version"]),
    ]
    for name, label, expected in cases:
        failures = check(name, "0" * 40, label)
        if not failures or expected[0] not in failures[0]:
            print(
                f"self-test: `{label}` was accepted as a checkable version",
                file=sys.stderr,
            )
            return 1
    for label in ("v1.0.5", "v0.24.0", "v4.2.2"):
        if not EXACT_VERSION.match(label):
            print(f"self-test: `{label}` was refused as a version", file=sys.stderr)
            return 1
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--best-effort",
        action="store_true",
        help="report and pass when the API cannot be read, whatever the reason; "
        "for a laptop, never for CI",
    )
    arguments = parser.parse_args()

    if self_test() != 0:
        return 1

    failures: list[str] = []
    for name, (digest, label) in sorted(PINS.items()):
        try:
            failures.extend(check(name, digest, label))
        except (Refused, urllib.error.URLError, TimeoutError) as error:
            if arguments.best_effort:
                # NOTE: Said on the way past rather than folded into the final line, because the run passed and did not check anything,
                # NOTE: and a reader who sees only the count would not know.
                print(f"not checked: {name} could not be read ({error})")
                return 0
            print(
                f"{name}: could not read the GitHub API ({error}). This is the "
                f"only check that can see an untrue pin, so it fails rather than "
                f"passes when it cannot run.",
                file=sys.stderr,
            )
            return 1

    if failures:
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1
    exact = sum(1 for _, label in PINS.values() if EXACT_VERSION.match(label))
    print(
        f"{exact} action pins are the commit their version tag names upstream, "
        f"and {len(UNTAGGED)} publishes no version tags and is recorded as such"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
