#!/usr/bin/env python3
"""Exercise stable release-document version synchronization."""

from __future__ import annotations

import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import sync_release_docs


class SyncReleaseDocsTests(unittest.TestCase):
    def fixture(self, version: str = "0.2.0") -> pathlib.Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = pathlib.Path(temporary.name)
        (root / "rust").mkdir()
        (root / "rust/Cargo.toml").write_text(
            f'[workspace]\n[workspace.package]\nversion = "{version}"\n',
            encoding="utf-8",
        )
        examples = {
            "README.md": "P4suta/OComment@v0.1.0\n",
            "docs/ci.md": "rev: v0.1.0\n",
            "docs/docker.md": "ghcr.io/p4suta/ocomment:0.1.0\n",
            "docs/installation.md": "gh release download v0.1.0\n",
            "docs/verify.md": "refs/tags/v0.1.0\n",
        }
        for relative, text in examples.items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        return root

    def test_reports_stale_pins(self) -> None:
        problems = sync_release_docs.failures(self.fixture())
        self.assertEqual(len(problems), len(sync_release_docs.PINNED_DOCS))
        self.assertTrue(all("expected workspace version 0.2.0" in item for item in problems))

    def test_synchronizes_every_document(self) -> None:
        root = self.fixture()
        self.assertEqual(sync_release_docs.synchronize(root), len(sync_release_docs.PINNED_DOCS))
        self.assertEqual(sync_release_docs.failures(root), [])
        for relative in sync_release_docs.PINNED_DOCS:
            text = (root / relative).read_text(encoding="utf-8")
            self.assertIn("0.2.0", text)
            self.assertNotIn("0.1.0", text)

    def test_rejects_a_document_without_a_pin(self) -> None:
        root = self.fixture(version="0.1.0")
        (root / "docs/verify.md").write_text("no release example\n", encoding="utf-8")
        self.assertIn(
            "docs/verify.md has no stable OComment version pin",
            sync_release_docs.failures(root),
        )


if __name__ == "__main__":
    unittest.main()
