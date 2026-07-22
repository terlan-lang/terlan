#!/usr/bin/env python3
"""Focused tests for generated release-manifest surface contracts."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("check_release_manifest.py")
SPEC = importlib.util.spec_from_file_location("check_release_manifest", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
CHECKER = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = CHECKER
SPEC.loader.exec_module(CHECKER)


class ReleaseManifestGeneratedContractTest(unittest.TestCase):
    """Exercise generated and executable API row classification."""

    def test_generated_surface_contract_is_public_and_unannotated(self) -> None:
        source = "module std.js.ArrayTest.\n\npub generated_surface_contract(): Bool -> true.\n"
        self.assertTrue(CHECKER.is_generated_surface_api("std.js.Array.generated_surface"))
        self.assertFalse(CHECKER.source_has_annotated_test(source, "generated_surface_contract"))

    def test_generated_surface_contract_rejects_test_annotation(self) -> None:
        source = (
            "module std.js.ArrayTest.\n\n"
            "@test\n"
            "pub generated_surface_contract(): Bool -> true.\n"
        )
        self.assertTrue(CHECKER.source_has_annotated_test(source, "generated_surface_contract"))

    def test_public_function_requires_exact_name(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "ArrayTest.terl"
            path.write_text("pub generated_surface_contract_extra(): Bool -> true.\n")
            self.assertFalse(CHECKER.has_public_function(path, "generated_surface_contract"))


if __name__ == "__main__":
    unittest.main()
