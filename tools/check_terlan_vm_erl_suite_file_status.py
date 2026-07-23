#!/usr/bin/env python3
"""Adversarial tests for the BEAM test-suite file status ledger."""

from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from terlan_vm_erl_suite_file_status import (
    DeletionManifestRow,
    FileStatusRow,
    audit_deletion_manifest,
    audit_file_status,
    expected_file_status_summary,
    read_deletion_manifest,
    read_file_status,
)


def row(
    path: str,
    port_status: str = "not-ported",
    evidence: str = "-",
    line: int = 2,
) -> FileStatusRow:
    return FileStatusRow(path, port_status, evidence, line)


def deleted_row(
    path: str,
    classification: str = "port-to-rust-vm-test",
    gate: str = "vm-check",
    generation: str = "2026-07-21",
    line: int = 2,
) -> DeletionManifestRow:
    return DeletionManifestRow(path, classification, gate, generation, line)


class FileStatusTest(unittest.TestCase):
    def test_reader_rejects_malformed_rows(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            status_path = root / "status.tsv"
            status_path.write_text("only\ttwo\n", encoding="utf-8")

            rows, findings = read_file_status(status_path, root)

        self.assertEqual(rows, [])
        self.assertEqual(
            findings,
            ["status.tsv:1: expected 3 TSV fields, found 2"],
        )

    def test_valid_active_progress_is_counted(self) -> None:
        rows = [
            row("terlan-vm/a.erl", "ported", "vm-check"),
            row("terlan-vm/b.erl", line=3),
        ]

        findings = audit_file_status(
            rows,
            ["terlan-vm/a.erl", "terlan-vm/b.erl"],
            {"terlan-vm/a.erl": "vm-check", "terlan-vm/b.erl": "vm-check"},
            {"vm-check"},
            Path("status.tsv"),
            Path("."),
        )

        self.assertEqual(findings, [])
        self.assertEqual(
            expected_file_status_summary(rows),
            {
                ("total", "active-files"): 2,
                ("port_status", "not-ported"): 1,
                ("port_status", "ported"): 1,
            },
        )

    def test_deletion_manifest_accepts_replaced_and_nonportable_tombstones(self) -> None:
        rows = [
            deleted_row("terlan-vm/a.erl"),
            deleted_row(
                "terlan-vm/beam_only.rs",
                "remove-non-portable",
                "-",
                line=3,
            ),
        ]

        findings = audit_deletion_manifest(
            rows,
            [],
            set(),
            {
                "terlan-vm/a.erl": "port-to-rust-vm-test",
                "terlan-vm/beam_only.rs": "remove-non-portable",
            },
            {"terlan-vm/a.erl": "vm-check", "terlan-vm/beam_only.rs": ""},
            {"vm-check"},
            Path("deletions.tsv"),
            Path("."),
        )

        self.assertEqual(findings, [])

    def test_deletion_manifest_reader_and_reintroduction_guard(self) -> None:
        with TemporaryDirectory() as directory:
            root = Path(directory)
            manifest_path = root / "deletions.tsv"
            manifest_path.write_text(
                "terlan-vm/a.erl\tport-to-rust-vm-test\tvm-check\t2026-07-21\n",
                encoding="utf-8",
            )
            rows, findings = read_deletion_manifest(manifest_path, root)

        self.assertEqual(findings, [])
        self.assertEqual(rows[0].corpus_generation, "2026-07-21")
        findings = audit_deletion_manifest(
            rows,
            ["terlan-vm/a.erl"],
            set(),
            {"terlan-vm/a.erl": "port-to-rust-vm-test"},
            {"terlan-vm/a.erl": "vm-check"},
            {"vm-check"},
            Path("deletions.tsv"),
            Path("."),
        )
        self.assertTrue(any("reintroduced" in finding for finding in findings), findings)

    def test_audit_rejects_false_or_unproven_progress(self) -> None:
        cases = [
            (
                [row("terlan-vm/a.erl", "ported")],
                ["terlan-vm/a.erl"],
                {"terlan-vm/a.erl": "vm-check"},
                {"vm-check"},
                "ported file requires replacement evidence",
            ),
            (
                [row("terlan-vm/a.erl", "ported", "other-check")],
                ["terlan-vm/a.erl"],
                {"terlan-vm/a.erl": "vm-check"},
                {"vm-check", "other-check"},
                "does not match inventory gate `vm-check`",
            ),
            (
                [row("terlan-vm/a.erl", "ported", "vm-check")],
                ["terlan-vm/a.erl"],
                {"terlan-vm/a.erl": "vm-check"},
                set(),
                "is not a Make target",
            ),
            (
                [row("terlan-vm/a.erl")],
                [],
                {"terlan-vm/a.erl": "vm-check"},
                {"vm-check"},
                "active file is absent",
            ),
        ]
        for rows, files, gates, targets, expected in cases:
            with self.subTest(expected=expected):
                findings = audit_file_status(
                    rows,
                    files,
                    gates,
                    targets,
                    Path("status.tsv"),
                    Path("."),
                )
                self.assertTrue(any(expected in finding for finding in findings), findings)

    def test_audit_rejects_missing_duplicate_and_unsorted_rows(self) -> None:
        rows = [
            row("terlan-vm/b.erl"),
            row("terlan-vm/b.erl", line=3),
            row("terlan-vm/a.erl", line=4),
        ]
        findings = audit_file_status(
            rows,
            ["terlan-vm/a.erl", "terlan-vm/b.erl", "terlan-vm/c.erl"],
            {
                "terlan-vm/a.erl": "vm-check",
                "terlan-vm/b.erl": "vm-check",
                "terlan-vm/c.erl": "vm-check",
            },
            set(),
            Path("status.tsv"),
            Path("."),
        )

        self.assertTrue(any("rows must be sorted" in finding for finding in findings))
        self.assertTrue(any("duplicate file status" in finding for finding in findings))
        self.assertIn(
            "terlan-vm/c.erl: missing file-level migration status",
            findings,
        )


if __name__ == "__main__":
    unittest.main()
