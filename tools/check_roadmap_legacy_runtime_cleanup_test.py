#!/usr/bin/env python3
"""Regression tests for active roadmap root selection."""

from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from check_roadmap_legacy_runtime_cleanup import select_roadmap_root


class RoadmapRootTest(unittest.TestCase):
    def test_skips_directory_without_active_roadmap(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            inactive = root / "repository" / "docs" / "roadmap"
            active = root / "workspace" / "docs" / "roadmap"
            inactive.mkdir(parents=True)
            active.mkdir(parents=True)
            (inactive / "RELEASE_NOTES_0_0_7.md").write_text("done\n", encoding="utf-8")
            (active / "ROADMAP_0_0_7.md").write_text("active\n", encoding="utf-8")

            selected = select_roadmap_root((inactive, active))

            self.assertEqual(selected, active)


if __name__ == "__main__":
    unittest.main()
