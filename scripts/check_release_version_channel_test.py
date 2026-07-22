#!/usr/bin/env python3
"""Adversarial tests for check_release_version_channel.py."""

from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CHECKER = ROOT / "scripts/check_release_version_channel.py"


class ReleaseVersionChannelTest(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        paths = (
            "Cargo.toml", "README.md", "CHANGELOG.md", "install.sh", "install.ps1",
            "crates/terlan/Cargo.toml", "crates/terlan/src/main.rs",
            "crates/terlan/src/vm/main_part_001.rs",
            "std/manifest.toml", "editors/vscode/package.json", "editors/vscode/package-lock.json",
            "tree-sitter-terlan/package.json", "tree-sitter-terlan/package-lock.json",
            "editors/intellij/build.gradle.kts",
        )
        for relative in paths:
            source = ROOT / relative
            target = self.root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, target)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def check(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [str(CHECKER), "--root", str(self.root), "--report", str(self.root / "report.json"), *args],
            capture_output=True, text=True, check=False,
        )

    def replace(self, relative: str, old: str, new: str) -> None:
        path = self.root / relative
        path.write_text(path.read_text(encoding="utf-8").replace(old, new), encoding="utf-8")

    def test_current_fixture_passes_and_writes_report(self) -> None:
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stderr)
        report = json.loads((self.root / "report.json").read_text(encoding="utf-8"))
        self.assertEqual(report["status"], "ok")
        self.assertEqual(report["artifactFilenameCoverage"], 5)

    def test_stale_package_and_editor_versions_fail(self) -> None:
        self.replace("tree-sitter-terlan/package.json", '"version": "0.0.7"', '"version": "0.0.6"')
        self.replace("editors/intellij/build.gradle.kts", 'version = "0.0.7"', 'version = "0.0.6"')
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("tree-sitter-terlan/package.json:version", result.stderr)
        self.assertIn("editors/intellij/build.gradle.kts:version", result.stderr)

    def test_stale_installer_docs_and_release_notes_fail(self) -> None:
        self.replace("install.sh", "v0.0.7", "v0.0.6")
        self.replace("install.ps1", "github.com/terlan-lang/terlan", "example.invalid/terlan")
        self.replace("README.md", "Current version: `0.0.7`.", "Current version: `0.0.6`.")
        self.replace("CHANGELOG.md", "## 0.0.7", "## 0.0.6")
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("install.sh:default_tag", result.stderr)
        self.assertIn("install.ps1:release_base_url", result.stderr)
        self.assertIn("README.md:current_version", result.stderr)
        self.assertIn("CHANGELOG.md:release_heading", result.stderr)

    def test_tag_and_stable_prerelease_mismatches_fail(self) -> None:
        wrong_tag = self.check("--tag", "v0.0.6")
        self.assertNotEqual(wrong_tag.returncode, 0)
        self.assertIn("<release>:tag", wrong_tag.stderr)
        self.replace("Cargo.toml", 'version = "0.0.7"', 'version = "0.0.7-rc.1"')
        result = self.check("--channel", "stable")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("<release>:channel", result.stderr)

    def test_shadow_compiler_version_fails(self) -> None:
        compiler = self.root / "terlc"
        compiler.write_text("#!/bin/sh\necho 'terlc 0.0.6'\n", encoding="utf-8")
        compiler.chmod(0o755)
        result = self.check("--compiler", str(compiler))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("compiler_binary_version", result.stderr)

    def test_missing_vm_runtime_version_source_fails(self) -> None:
        self.replace(
            "crates/terlan/src/vm/main_part_001.rs",
            'env!("CARGO_PKG_VERSION")',
            '"stale-version"',
        )
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("crates/terlan/src/vm/main_part_001.rs:runtime_version_source", result.stderr)

    def test_write_updates_supported_metadata(self) -> None:
        result = self.check("0.0.8", "--write")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('version = "0.0.8"', (self.root / "Cargo.toml").read_text(encoding="utf-8"))
        tree_version = json.loads(
            (self.root / "tree-sitter-terlan/package.json").read_text(encoding="utf-8")
        )["version"]
        self.assertEqual(tree_version, "0.0.8")


if __name__ == "__main__":
    unittest.main()
