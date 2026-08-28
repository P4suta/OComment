#!/usr/bin/env python3
"""Keep the canonical editor IDs, VS Code selectors, and LSP aliases aligned."""

from __future__ import annotations

import json
import pathlib
import sys
import tomllib


ROOT = pathlib.Path(__file__).resolve().parents[1]


def main() -> int:
    with (ROOT / "spec/languages.toml").open("rb") as stream:
        languages = tomllib.load(stream)["languages"]
    identifiers = [identifier for row in languages for identifier in row["editor_ids"]]
    failures = []
    if len(identifiers) != 35:
        failures.append(f"the v0.1 editor ID set has {len(identifiers)} entries, expected 35")
    duplicates = sorted({identifier for identifier in identifiers if identifiers.count(identifier) > 1})
    if duplicates:
        failures.append(f"editor IDs belong to multiple languages: {', '.join(duplicates)}")
    for row in languages:
        if not row["editor_ids"]:
            failures.append(f"{row['name']} has no editor_ids")

    manifest = json.loads((ROOT / "editors/vscode/package.json").read_text(encoding="utf-8"))
    configured = manifest["contributes"]["configuration"]["properties"][
        "ocomment.languages"
    ]["default"]
    activated = [
        event.removeprefix("onLanguage:")
        for event in manifest["activationEvents"]
        if event.startswith("onLanguage:")
    ]
    if set(configured) != set(identifiers):
        failures.append("VS Code ocomment.languages differs from canonical editor_ids")
    if set(activated) != set(identifiers):
        failures.append("VS Code activationEvents differs from canonical editor_ids")
    if configured != activated:
        failures.append("VS Code activationEvents and ocomment.languages use different orders")

    canonical_names = {row["name"] for row in languages}
    aliases = {identifier for identifier in identifiers if identifier not in canonical_names}
    lsp = (ROOT / "rust/ocomment/src/lsp.rs").read_text(encoding="utf-8")
    try:
        mapping = lsp.split("fn language_from_lsp", 1)[1].split("\n}", 1)[0]
    except IndexError:
        failures.append("rust/ocomment/src/lsp.rs has no readable language_from_lsp mapping")
    else:
        for alias in sorted(aliases):
            if f'"{alias}" =>' not in mapping:
                failures.append(f"LSP has no explicit mapping for editor alias {alias!r}")

    if failures:
        print("\n".join(failures), file=sys.stderr)
        return 1
    print(f"30 languages, {len(identifiers)} editor IDs, VS Code, and LSP aliases agree")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
