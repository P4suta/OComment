#!/usr/bin/env python3
"""Fail when .pre-commit-hooks.yaml drifts from the canonical language table.

The `files:` pattern of every published pre-commit hook must select exactly the
extensions in `spec/languages.toml`, so adding a language to the shared spec
cannot silently leave the hooks scanning the old file set. Only the standard
library is used, because this runs next to `tools/check_embedded_specs.py` in a
job that installs nothing.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import tomllib


ROOT = pathlib.Path(__file__).resolve().parents[1]
LANGUAGES = ROOT / "spec/languages.toml"
HOOKS = ROOT / ".pre-commit-hooks.yaml"

EXPECTED_ENTRIES = {
    "ocomment-check": "ocomment check",
    "ocomment-fix": "ocomment fix",
}

# INVARIANT: pre-commit rejects an unknown manifest key only when a consumer runs
# INVARIANT: the hook, so a typo in the set below ships broken. These are the
# INVARIANT: keys its manifest schema takes.
KNOWN_FIELDS = frozenset(
    {
        "additional_dependencies",
        "alias",
        "always_run",
        "args",
        "description",
        "entry",
        "exclude",
        "exclude_types",
        "fail_fast",
        "files",
        "id",
        "language",
        "language_version",
        "log_file",
        "minimum_pre_commit_version",
        "name",
        "pass_filenames",
        "require_serial",
        "stages",
        "types",
        "types_or",
        "verbose",
    }
)
REQUIRED_FIELDS = ("id", "name", "entry", "language")


def expected_files_pattern() -> str:
    """Build the `files:` regex that matches every extension in the spec."""
    with LANGUAGES.open("rb") as stream:
        table = tomllib.load(stream)
    extensions: set[str] = set()
    for language in table["languages"]:
        for extension in language["extensions"]:
            if not re.fullmatch(r"[a-z0-9]+", extension):
                raise SystemExit(
                    f"extension {extension!r} needs regex quoting; update {__file__}"
                )
            extensions.add(extension)
    if not extensions:
        raise SystemExit(f"{LANGUAGES} lists no extensions")
    return r"(?i)\.(" + "|".join(sorted(extensions)) + r")$"


def unquote(value: str) -> str:
    """Undo the single- or double-quoted YAML scalar forms this file uses."""
    if len(value) >= 2 and value[0] == value[-1] and value[0] in "'\"":
        inner = value[1:-1]
        return inner.replace("''", "'") if value[0] == "'" else inner
    return value


def parse_hooks(text: str) -> list[dict[str, str]]:
    """Parse the flat `- key: value` hook list without a YAML dependency."""
    hooks: list[dict[str, str]] = []
    for number, line in enumerate(text.splitlines(), start=1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if line.startswith("- "):
            hooks.append({})
            stripped = stripped[2:].strip()
            if not stripped:
                continue
        elif not line.startswith("  "):
            raise SystemExit(f"{HOOKS.name}:{number}: unexpected top-level line")
        if not hooks:
            raise SystemExit(f"{HOOKS.name}:{number}: value outside a hook entry")
        key, separator, value = stripped.partition(":")
        if not separator:
            raise SystemExit(f"{HOOKS.name}:{number}: expected `key: value`")
        hooks[-1][key.strip()] = unquote(value.strip())
    return hooks


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--print-pattern",
        action="store_true",
        help="write the expected `files:` regex to stdout and exit",
    )
    args = parser.parse_args()

    pattern = expected_files_pattern()
    if args.print_pattern:
        print(pattern)
        return 0

    if not HOOKS.is_file():
        print(f"{HOOKS.relative_to(ROOT)} is missing")
        return 1

    failures: list[str] = []
    parsed = parse_hooks(HOOKS.read_text())
    for position, hook in enumerate(parsed):
        label = hook.get("id") or f"#{position}"
        for field in sorted(set(hook) - KNOWN_FIELDS):
            failures.append(f"hook `{label}` has `{field}`, which pre-commit does not define")
        for field in REQUIRED_FIELDS:
            if not hook.get(field):
                failures.append(f"hook `{label}` is missing the required `{field}`")

    hooks = {hook.get("id", ""): hook for hook in parsed}
    for hook_id, entry in EXPECTED_ENTRIES.items():
        hook = hooks.get(hook_id)
        if hook is None:
            failures.append(f"hook `{hook_id}` is missing")
            continue
        if hook.get("entry") != entry:
            failures.append(
                f"hook `{hook_id}` entry is {hook.get('entry')!r}, expected {entry!r}"
            )
        if hook.get("language") != "system":
            failures.append(
                f"hook `{hook_id}` language is {hook.get('language')!r}, expected 'system'"
                " (pre-commit's `language: rust` builds the checkout root, and the"
                " manifest lives in rust/)"
            )
        actual = hook.get("files")
        if actual != pattern:
            failures.append(
                f"hook `{hook_id}` files is {actual!r}\n"
                f"{' ' * 4}spec/languages.toml requires {pattern!r}"
            )

    if failures:
        print("\n".join(failures))
        print("Regenerate with: python3 tools/check_hooks.py --print-pattern")
        return 1
    print(f"{len(EXPECTED_ENTRIES)} pre-commit hooks match spec/languages.toml")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
