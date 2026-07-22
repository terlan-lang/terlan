#!/usr/bin/env python3
"""Adversarial tests for external VM suite inventory overrides."""

import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from check_terlan_vm_erl_suite_audit import (
    InventoryRow,
    PortPlanRow,
    PortStatusRow,
    ROOT,
    audit,
    audit_external_make_test_targets,
    audit_port_plan,
    audit_port_status,
    matching_rows,
    read_port_status,
)


def inventory(
    pattern: str,
    classification: str,
    gate: str,
    line: int,
) -> InventoryRow:
    return InventoryRow(pattern, classification, gate, "erts-rust", "test row", line)


class SuiteInventoryOverrideTest(unittest.TestCase):
    def test_port_status_reader_requires_execution_evidence_columns(self) -> None:
        with TemporaryDirectory(dir=ROOT) as directory:
            path = Path(directory) / "status.tsv"
            path.write_text(
                "scheduler\tP0\tport-to-rust-vm-test\tvm-check\t"
                "rust-runtime\tvm-check\tpartial\tnot-deleted\tnotes\n",
                encoding="utf-8",
            )
            rows, findings = read_port_status(path)

            self.assertEqual(findings, [])
            self.assertEqual(rows[0].execution_path, "rust-runtime")
            self.assertEqual(rows[0].execution_gate, "vm-check")

            path.write_text(
                "scheduler\tP0\tport-to-rust-vm-test\tvm-check\t"
                "partial\tnot-deleted\tnotes\n",
                encoding="utf-8",
            )
            rows, findings = read_port_status(path)

            self.assertEqual(rows, [])
            self.assertTrue(any("expected 9 TSV fields" in finding for finding in findings))

    def test_port_status_rejects_unproven_native_aot_claim(self) -> None:
        plan = PortPlanRow(
            priority="P0",
            area="scheduler",
            source_patterns=("terlan-vm/erts/**",),
            classification="port-to-rust-vm-test",
            replacement_gate="vm-check",
            first_port_action="port scheduler behavior",
            delete_rule="delete after equivalent coverage",
            line=2,
        )
        status = PortStatusRow(
            area="scheduler",
            priority="P0",
            classification="port-to-rust-vm-test",
            replacement_gate="vm-check",
            execution_path="native-aot",
            execution_gate="vm-check",
            port_status="partial",
            deletion_status="not-deleted",
            notes="invalid native claim",
            line=2,
        )

        findings = audit_port_status([status], [plan], [], {"vm-check"})

        self.assertTrue(
            any("native-aot execution requires" in finding for finding in findings),
            findings,
        )

    def test_port_status_accepts_rust_runtime_execution_evidence(self) -> None:
        plan = PortPlanRow(
            priority="P0",
            area="scheduler",
            source_patterns=("terlan-vm/erts/**",),
            classification="port-to-rust-vm-test",
            replacement_gate="vm-check",
            first_port_action="port scheduler behavior",
            delete_rule="delete after equivalent coverage",
            line=2,
        )
        status = PortStatusRow(
            area="scheduler",
            priority="P0",
            classification="port-to-rust-vm-test",
            replacement_gate="vm-check",
            execution_path="rust-runtime",
            execution_gate="vm-check",
            port_status="partial",
            deletion_status="not-deleted",
            notes="runtime gate owns the replacement",
            line=2,
        )

        self.assertEqual(
            audit_port_status([status], [plan], [], {"vm-check"}),
            [],
        )

    def test_external_make_test_target_accepts_existing_source(self) -> None:
        makefile = (
            "check:\n"
            "\tcargo test --manifest-path erts/rust/terlan_vm/Cargo.toml "
            "--test runtime_semantics\n"
        )
        files = {"terlan-vm/erts/rust/terlan_vm/tests/runtime_semantics.rs"}

        self.assertEqual(audit_external_make_test_targets(makefile, files), [])

    def test_external_make_test_target_rejects_deleted_source(self) -> None:
        makefile = (
            "check:\n"
            "\tcargo test --manifest-path erts/rust/terlan_vm/Cargo.toml "
            "--test deleted_fixture\n"
        )

        self.assertEqual(
            audit_external_make_test_targets(makefile, set()),
            [
                "terlan-vm/GNUmakefile:2: integration test `deleted_fixture` has no source "
                "`terlan-vm/erts/rust/terlan_vm/tests/deleted_fixture.rs`"
            ],
        )

    def test_exact_path_override_takes_precedence_over_broad_glob(self) -> None:
        broad = inventory("terlan-vm/erts/rust/**", "port-to-rust-vm-test", "vm-check", 2)
        exact = inventory("terlan-vm/erts/rust/beam_only.rs", "remove-non-portable", "", 3)

        self.assertEqual(matching_rows(exact.pattern, [broad, exact]), [exact])
        self.assertEqual(
            matching_rows("terlan-vm/erts/rust/runtime.rs", [broad, exact]),
            [broad],
        )

    def test_deleted_nonportable_override_remains_auditable_tombstone(self) -> None:
        broad = inventory("terlan-vm/erts/rust/**", "port-to-rust-vm-test", "vm-check", 2)
        exact = inventory("terlan-vm/erts/rust/beam_only.rs", "remove-non-portable", "", 3)

        findings = audit(
            [broad, exact],
            ["terlan-vm/erts/rust/runtime.rs"],
            {"vm-check"},
            deleted_paths={exact.pattern},
        )

        self.assertEqual(findings, [])

    def test_deleted_equivalent_override_requires_checked_tombstone(self) -> None:
        broad = inventory("terlan-vm/erts/rust/**", "port-to-rust-vm-test", "vm-check", 2)
        exact = inventory(
            "terlan-vm/erts/rust/replaced.rs",
            "delete-after-vm-equivalent",
            "vm-check",
            3,
        )
        files = ["terlan-vm/erts/rust/runtime.rs"]

        self.assertEqual(
            audit(
                [broad, exact],
                files,
                {"vm-check"},
                deleted_paths={exact.pattern},
            ),
            [],
        )
        findings = audit([broad, exact], files, {"vm-check"})
        self.assertTrue(
            any("does not match any external test-suite file" in finding for finding in findings),
            findings,
        )

    def test_nonportable_override_fails_while_source_still_exists(self) -> None:
        broad = inventory("terlan-vm/erts/rust/**", "port-to-rust-vm-test", "vm-check", 2)
        exact = inventory("terlan-vm/erts/rust/beam_only.rs", "remove-non-portable", "", 3)

        findings = audit(
            [broad, exact],
            ["terlan-vm/erts/rust/runtime.rs", exact.pattern],
            {"vm-check"},
        )

        self.assertTrue(
            any("remove-non-portable files still exist" in finding for finding in findings),
            findings,
        )

    def test_port_plan_pattern_matches_deleted_sources(self) -> None:
        inventory_rows = [
            inventory("terlan-vm/erts/epmd/**", "delete-after-vm-equivalent", "vm-distribution-envelope-check", 2),
            inventory("terlan-vm/erts/**", "delete-after-vm-equivalent", "vm-check", 3),
        ]
        plan_rows = [
            PortPlanRow(
                priority="P1",
                area="distribution-framing",
                source_patterns=("terlan-vm/erts/epmd/**",),
                classification="delete-after-vm-equivalent",
                replacement_gate="vm-distribution-envelope-check",
                first_port_action="delete legacy framing tests",
                delete_rule="remove obsolete distribution framing artifacts",
                line=5,
            ),
            PortPlanRow(
                priority="P0",
                area="scheduler",
                source_patterns=("terlan-vm/erts/**",),
                classification="delete-after-vm-equivalent",
                replacement_gate="vm-check",
                first_port_action="placeholder coverage",
                delete_rule="placeholder rule",
                line=6,
            ),
            PortPlanRow(
                priority="P0",
                area="mailbox",
                source_patterns=("terlan-vm/erts/**",),
                classification="delete-after-vm-equivalent",
                replacement_gate="vm-check",
                first_port_action="placeholder coverage",
                delete_rule="placeholder rule",
                line=7,
            ),
            PortPlanRow(
                priority="P0",
                area="timers",
                source_patterns=("terlan-vm/erts/**",),
                classification="delete-after-vm-equivalent",
                replacement_gate="vm-check",
                first_port_action="placeholder coverage",
                delete_rule="placeholder rule",
                line=8,
            ),
            PortPlanRow(
                priority="P0",
                area="process-registry",
                source_patterns=("terlan-vm/erts/**",),
                classification="delete-after-vm-equivalent",
                replacement_gate="vm-check",
                first_port_action="placeholder coverage",
                delete_rule="placeholder rule",
                line=9,
            ),
            PortPlanRow(
                priority="P0",
                area="links-monitors",
                source_patterns=("terlan-vm/erts/**",),
                classification="delete-after-vm-equivalent",
                replacement_gate="vm-check",
                first_port_action="placeholder coverage",
                delete_rule="placeholder rule",
                line=10,
            ),
            PortPlanRow(
                priority="P1",
                area="serialization",
                source_patterns=("terlan-vm/erts/**",),
                classification="delete-after-vm-equivalent",
                replacement_gate="vm-check",
                first_port_action="placeholder coverage",
                delete_rule="placeholder rule",
                line=11,
            ),
            PortPlanRow(
                priority="P2",
                area="filesystem",
                source_patterns=("terlan-vm/erts/**",),
                classification="delete-after-vm-equivalent",
                replacement_gate="vm-check",
                first_port_action="placeholder coverage",
                delete_rule="placeholder rule",
                line=12,
            ),
            PortPlanRow(
                priority="P2",
                area="std-behavior",
                source_patterns=("terlan-vm/erts/**",),
                classification="delete-after-vm-equivalent",
                replacement_gate="vm-check",
                first_port_action="placeholder coverage",
                delete_rule="placeholder rule",
                line=13,
            ),
            PortPlanRow(
                priority="P1",
                area="http-tcp",
                source_patterns=("terlan-vm/erts/**",),
                classification="delete-after-vm-equivalent",
                replacement_gate="vm-check",
                first_port_action="placeholder coverage",
                delete_rule="placeholder rule",
                line=14,
            ),
            PortPlanRow(
                priority="P1",
                area="epmd-discovery",
                source_patterns=("terlan-vm/erts/**",),
                classification="delete-after-vm-equivalent",
                replacement_gate="vm-check",
                first_port_action="placeholder coverage",
                delete_rule="placeholder rule",
                line=15,
            ),
        ]
        deleted_paths = {
            "terlan-vm/erts/epmd/make_test_dir/epmd_test/epmd_SUITE.erl",
            "terlan-vm/erts/epmd/test/epmd_SUITE.erl",
        }
        self.assertEqual(
            audit_port_plan(
                plan_rows,
                inventory_rows,
                ["terlan-vm/erts/emulator/test/distribution_SUITE.erl"],
                {"vm-distribution-envelope-check", "vm-check"},
                deleted_paths=deleted_paths,
            ),
            [],
        )


if __name__ == "__main__":
    unittest.main()
