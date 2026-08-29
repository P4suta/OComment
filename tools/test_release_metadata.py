#!/usr/bin/env python3
"""Negative tests for the release metadata gate."""

from __future__ import annotations

import pathlib
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import release_metadata


class ReleaseMetadataTests(unittest.TestCase):
    def fixture(self) -> pathlib.Path:
        temporary = tempfile.TemporaryDirectory(prefix="ocomment-release-metadata-")
        self.addCleanup(temporary.cleanup)
        root = pathlib.Path(temporary.name)
        (root / "rust/ocomment-core").mkdir(parents=True)
        (root / "rust/ocomment-plugin-sdk").mkdir()
        (root / "rust/ocomment").mkdir()
        (root / "rust/Cargo.toml").write_text(
            '[workspace]\nmembers = []\n[workspace.package]\nversion = "0.1.0"\n',
            encoding="utf-8",
        )
        for name, relative in release_metadata.PUBLIC_CRATES:
            (root / relative).write_text(
                f'[package]\nname = "{name}"\nversion.workspace = true\n',
                encoding="utf-8",
            )
        packages = "".join(
            f'[[package]]\nname = "{name}"\nversion = "0.1.0"\n'
            for name, _ in release_metadata.PUBLIC_CRATES
        )
        (root / "rust/Cargo.lock").write_text(
            f"version = 4\n{packages}", encoding="utf-8"
        )
        (root / "CHANGELOG.md").write_text("## 0.1.0\n", encoding="utf-8")
        return root

    def test_complete_fixture_passes(self) -> None:
        self.assertEqual(release_metadata.metadata_failures(self.fixture(), "0.1.0"), [])

    def test_version_mismatch_is_rejected(self) -> None:
        failures = release_metadata.metadata_failures(self.fixture(), "0.2.0")
        self.assertTrue(any("workspace version" in failure for failure in failures))
        self.assertTrue(any("Cargo.lock" in failure for failure in failures))

    def test_missing_cli_changelog_entry_is_rejected(self) -> None:
        root = self.fixture()
        (root / "CHANGELOG.md").write_text("## Unreleased\n", encoding="utf-8")
        failures = release_metadata.metadata_failures(root, "0.1.0")
        self.assertIn("CHANGELOG.md has no 0.1.0 release heading", failures)

    def test_linked_keep_a_changelog_heading_is_accepted(self) -> None:
        root = self.fixture()
        (root / "CHANGELOG.md").write_text(
            "## [0.1.0](https://example.test/v0.1.0) - 2026-08-28\n",
            encoding="utf-8",
        )
        self.assertEqual(release_metadata.metadata_failures(root, "0.1.0"), [])

    def test_vscode_metadata_is_not_a_cli_release_input(self) -> None:
        root = self.fixture()
        extension = root / "editors/vscode"
        extension.mkdir(parents=True)
        (extension / "package.json").write_text(
            '{"version":"9.9.9"}\n', encoding="utf-8"
        )
        (extension / "CHANGELOG.md").write_text(
            "# No CLI release heading here\n", encoding="utf-8"
        )
        self.assertEqual(release_metadata.metadata_failures(root, "0.1.0"), [])

    def test_side_branch_tag_is_rejected(self) -> None:
        root = self.fixture()
        subprocess.run(["git", "init", "-q", root], check=True)
        subprocess.run(["git", "-C", root, "config", "user.name", "test"], check=True)
        subprocess.run(
            ["git", "-C", root, "config", "user.email", "test@example.invalid"],
            check=True,
        )
        subprocess.run(
            ["git", "-C", root, "config", "commit.gpgsign", "false"], check=True
        )
        subprocess.run(
            ["git", "-C", root, "config", "tag.gpgsign", "false"], check=True
        )
        no_hooks = root / ".git/no-hooks"
        no_hooks.mkdir()
        subprocess.run(
            ["git", "-C", root, "config", "core.hooksPath", str(no_hooks)],
            check=True,
        )
        subprocess.run(["git", "-C", root, "add", "."], check=True)
        subprocess.run(["git", "-C", root, "commit", "-qm", "main"], check=True)
        main = subprocess.check_output(
            ["git", "-C", root, "rev-parse", "HEAD"], text=True
        ).strip()
        subprocess.run(
            ["git", "-C", root, "update-ref", "refs/remotes/origin/main", main],
            check=True,
        )
        (root / "side").write_text("side", encoding="utf-8")
        subprocess.run(["git", "-C", root, "add", "side"], check=True)
        subprocess.run(["git", "-C", root, "commit", "-qm", "side"], check=True)
        subprocess.run(["git", "-C", root, "tag", "v0.1.0"], check=True)
        failures = release_metadata.tag_failures(root, "v0.1.0")
        self.assertIn("tag v0.1.0 is not an ancestor of origin/main", failures)


if __name__ == "__main__":
    unittest.main()
