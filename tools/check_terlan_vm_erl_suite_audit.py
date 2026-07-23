#!/usr/bin/env python3
"""Audit external `terlan-vm` BEAM/OTP test-suite material.

Inputs:
- `TERLAN_VM_ROOT`, or the conventional sibling `../terlan-vm` checkout.
- `docs/runtime/TERLAN_VM_BEAM_TEST_SUITE_INVENTORY.tsv`.
- `docs/runtime/TERLAN_VM_BEAM_TEST_PORT_PLAN.tsv`.
- `docs/runtime/TERLAN_VM_BEAM_TEST_PORT_STATUS.tsv`.
- `docs/runtime/TERLAN_VM_BEAM_TEST_FILE_STATUS.tsv`.
- `docs/runtime/TERLAN_VM_BEAM_TEST_DELETION_MANIFEST.tsv`.
- `docs/runtime/TERLAN_VM_BEAM_TEST_FILE_STATUS_SUMMARY.tsv`.
- `docs/runtime/TERLAN_VM_BEAM_TEST_PORT_PLAN_SUMMARY.tsv`.
- `docs/runtime/TERLAN_VM_BEAM_TEST_SUITE_SUMMARY.tsv`.
- The golden repo `Makefile` for replacement-gate validation.

Outputs:
- Exit status 0 when every discovered external test-suite file is classified,
  every active replacement gate exists, and deleted-file tombstones retain
  their historical replacement-gate identity.
- Exit status 1 with stable diagnostics for uncovered files, stale inventory
  rows, invalid classifications, missing active replacement gates, or
  still-present `remove-non-portable` files.

Transformation:
- Treats the external OTP-derived test corpus as migration input only. Any
  test Terlan can or should own must be assigned to a VM/Terlan replacement
  gate; non-portable OTP machinery must not remain as active test material
  once it is classified for removal.
"""

from __future__ import annotations

from dataclasses import dataclass
from fnmatch import fnmatchcase
import os
from pathlib import Path
import re
import sys

from makefile_contract import make_targets_from_paths
from terlan_vm_erl_suite_file_status import (
    DeletionManifestRow,
    FileStatusRow,
    audit_deletion_manifest,
    audit_file_status,
    expected_file_status_summary,
    read_deletion_manifest,
    read_file_status,
)


ROOT = Path(__file__).resolve().parents[1]
DEFAULT_VM_ROOT = ROOT.parent / "terlan-vm"
INVENTORY_PATH = ROOT / "docs" / "runtime" / "TERLAN_VM_BEAM_TEST_SUITE_INVENTORY.tsv"
PORT_PLAN_PATH = ROOT / "docs" / "runtime" / "TERLAN_VM_BEAM_TEST_PORT_PLAN.tsv"
PORT_STATUS_PATH = ROOT / "docs" / "runtime" / "TERLAN_VM_BEAM_TEST_PORT_STATUS.tsv"
FILE_STATUS_PATH = ROOT / "docs" / "runtime" / "TERLAN_VM_BEAM_TEST_FILE_STATUS.tsv"
DELETION_MANIFEST_PATH = (
    ROOT / "docs" / "runtime" / "TERLAN_VM_BEAM_TEST_DELETION_MANIFEST.tsv"
)
FILE_STATUS_SUMMARY_PATH = (
    ROOT / "docs" / "runtime" / "TERLAN_VM_BEAM_TEST_FILE_STATUS_SUMMARY.tsv"
)
PORT_PLAN_SUMMARY_PATH = ROOT / "docs" / "runtime" / "TERLAN_VM_BEAM_TEST_PORT_PLAN_SUMMARY.tsv"
SUMMARY_PATH = ROOT / "docs" / "runtime" / "TERLAN_VM_BEAM_TEST_SUITE_SUMMARY.tsv"
MAKEFILE_PATH = ROOT / "Makefile"
ALLOWED_CLASSIFICATIONS = {
    "port-to-rust-vm-test",
    "port-to-terlan-test",
    "delete-after-vm-equivalent",
    "remove-non-portable",
}
REQUIRES_REPLACEMENT_GATE = {
    "port-to-rust-vm-test",
    "port-to-terlan-test",
    "delete-after-vm-equivalent",
}
IGNORED_DIRS = {
    ".git",
    "_build",
    "target",
    "node_modules",
}
TEST_DIR_NAMES = {
    "test",
    "tests",
}
TEST_BASENAME_SUFFIXES = (
    "_SUITE.erl",
    "_test.erl",
    "_tests.erl",
)
TEST_EXTENSIONS_UNDER_TEST_DIR = (
    ".erl",
    ".hrl",
    ".src",
    ".app",
    ".appup",
    ".config",
    ".conf",
    ".script",
    ".sh",
    ".escript",
    ".mk",
    ".rs",
)
TEST_FILENAMES_UNDER_TEST_DIR = {
    "Makefile",
    "makefile",
    "rebar.config",
}

REQUIRED_PORT_AREAS = {
    "scheduler",
    "mailbox",
    "timers",
    "process-registry",
    "links-monitors",
    "serialization",
    "distribution-framing",
    "epmd-discovery",
    "http-tcp",
    "filesystem",
    "std-behavior",
}
ALLOWED_PRIORITIES = {"P0", "P1", "P2", "P3"}
ALLOWED_PORT_STATUSES = {"not-ported", "partial", "ported"}
ALLOWED_DELETION_STATUSES = {"not-deleted", "deleted"}
ALLOWED_EXECUTION_PATHS = {
    "not-proven",
    "rust-runtime",
    "native-aot",
}
NATIVE_AOT_PROOF_GATES = {
    "tvm-direct-aot-backend-check",
    "tvm-native-image-loader-check",
    "tvm-aot-consumer-check",
    "tvm-aot-runtime-transition-check",
    "no-tvm-json-runtime-check",
    "no-vmir-interpreter-check",
}


@dataclass(frozen=True)
class InventoryRow:
    """One migration classification row for external VM test files."""

    pattern: str
    classification: str
    replacement_gate: str
    owner: str
    notes: str
    line: int


@dataclass(frozen=True)
class PortPlanRow:
    """One prioritized migration plan row for a VM-owned behavior area."""

    priority: str
    area: str
    source_patterns: tuple[str, ...]
    classification: str
    replacement_gate: str
    first_port_action: str
    delete_rule: str
    line: int


@dataclass(frozen=True)
class PortStatusRow:
    """Executable migration status for one planned VM test-suite port area."""

    area: str
    priority: str
    classification: str
    replacement_gate: str
    execution_path: str
    execution_gate: str
    port_status: str
    deletion_status: str
    notes: str
    line: int


def vm_root() -> Path:
    """Resolve the external `terlan-vm` checkout root."""

    return Path(os.environ.get("TERLAN_VM_ROOT", DEFAULT_VM_ROOT)).resolve()


def relative_vm_path(path: Path, root: Path) -> str:
    """Return the stable inventory path for a file below `terlan-vm`."""

    return (Path("terlan-vm") / path.relative_to(root)).as_posix()


def has_ignored_part(path: Path) -> bool:
    """Return whether a path is inside a generated or dependency directory."""

    return any(part in IGNORED_DIRS for part in path.parts)


def is_test_suite_file(path: Path, root: Path) -> bool:
    """Return whether a file is part of the external test-suite corpus."""

    rel_parts = path.relative_to(root).parts
    name = path.name
    if name.endswith(TEST_BASENAME_SUFFIXES):
        return True
    if not any(part in TEST_DIR_NAMES for part in rel_parts):
        return False
    if name in TEST_FILENAMES_UNDER_TEST_DIR:
        return True
    return name.endswith(TEST_EXTENSIONS_UNDER_TEST_DIR)


def discover_test_suite_files(root: Path) -> list[str]:
    """Return sorted external test-suite files requiring migration status."""

    files: list[str] = []
    for current_root, directory_names, file_names in os.walk(root):
        directory_names[:] = [
            name for name in directory_names if name not in IGNORED_DIRS
        ]
        current = Path(current_root)
        for file_name in file_names:
            path = current / file_name
            if is_test_suite_file(path, root):
                files.append(relative_vm_path(path, root))
    return sorted(files)


def audit_external_make_test_targets(
    makefile_text: str,
    discovered_files: set[str],
) -> list[str]:
    """Reject standalone VM integration-test commands whose source is absent."""

    findings: list[str] = []
    manifest = "erts/rust/terlan_vm/Cargo.toml"
    for line_number, line in enumerate(makefile_text.splitlines(), start=1):
        if manifest not in line:
            continue
        for test_name in re.findall(r"--test\s+([A-Za-z0-9_-]+)", line):
            source_path = f"terlan-vm/erts/rust/terlan_vm/tests/{test_name}.rs"
            if source_path not in discovered_files:
                findings.append(
                    "terlan-vm/GNUmakefile:"
                    f"{line_number}: integration test `{test_name}` has no source `{source_path}`"
                )
    return findings


def read_inventory(path: Path) -> tuple[list[InventoryRow], list[str]]:
    """Read the TSV inventory and return rows plus format diagnostics."""

    rows: list[InventoryRow] = []
    findings: list[str] = []
    if not path.is_file():
        return rows, [f"{path.relative_to(ROOT)}: missing inventory file"]
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        parts = raw_line.split("\t")
        if len(parts) != 5:
            findings.append(
                f"{path.relative_to(ROOT)}:{line_number}: expected 5 TSV fields, found {len(parts)}"
            )
            continue
        pattern, classification, replacement_gate, owner, notes = parts
        rows.append(
            InventoryRow(
                pattern=pattern,
                classification=classification,
                replacement_gate=replacement_gate,
                owner=owner,
                notes=notes,
                line=line_number,
            )
        )
    return rows, findings


def read_port_plan(path: Path) -> tuple[list[PortPlanRow], list[str]]:
    """Read the prioritized port plan and return rows plus diagnostics."""

    rows: list[PortPlanRow] = []
    findings: list[str] = []
    if not path.is_file():
        return rows, [f"{path.relative_to(ROOT)}: missing port plan file"]
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        parts = raw_line.split("\t")
        if len(parts) != 7:
            findings.append(
                f"{path.relative_to(ROOT)}:{line_number}: expected 7 TSV fields, found {len(parts)}"
            )
            continue
        priority, area, source_patterns, classification, replacement_gate, action, delete_rule = parts
        rows.append(
            PortPlanRow(
                priority=priority,
                area=area,
                source_patterns=tuple(
                    pattern.strip() for pattern in source_patterns.split(",") if pattern.strip()
                ),
                classification=classification,
                replacement_gate=replacement_gate,
                first_port_action=action,
                delete_rule=delete_rule,
                line=line_number,
            )
        )
    return rows, findings


def read_port_status(path: Path) -> tuple[list[PortStatusRow], list[str]]:
    """Read executable port/deletion status rows plus diagnostics."""

    rows: list[PortStatusRow] = []
    findings: list[str] = []
    if not path.is_file():
        return rows, [f"{path.relative_to(ROOT)}: missing port status file"]
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        parts = raw_line.split("\t")
        if len(parts) != 9:
            findings.append(
                f"{path.relative_to(ROOT)}:{line_number}: expected 9 TSV fields, found {len(parts)}"
            )
            continue
        (
            area,
            priority,
            classification,
            replacement_gate,
            execution_path,
            execution_gate,
            port_status,
            deletion_status,
            notes,
        ) = parts
        rows.append(
            PortStatusRow(
                area=area,
                priority=priority,
                classification=classification,
                replacement_gate=replacement_gate,
                execution_path=execution_path,
                execution_gate=execution_gate,
                port_status=port_status,
                deletion_status=deletion_status,
                notes=notes,
                line=line_number,
            )
        )
    return rows, findings


def read_summary(path: Path) -> tuple[dict[tuple[str, str], int], list[str]]:
    """Read checked summary counts for the external VM test-suite audit."""

    rows: dict[tuple[str, str], int] = {}
    findings: list[str] = []
    if not path.is_file():
        return rows, [f"{path.relative_to(ROOT)}: missing summary file"]
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        parts = raw_line.split("\t")
        if len(parts) != 3:
            findings.append(
                f"{path.relative_to(ROOT)}:{line_number}: expected 3 TSV fields, found {len(parts)}"
            )
            continue
        kind, name, raw_count = parts
        try:
            count = int(raw_count)
        except ValueError:
            findings.append(
                f"{path.relative_to(ROOT)}:{line_number}: count must be an integer, found `{raw_count}`"
            )
            continue
        key = (kind, name)
        if key in rows:
            findings.append(
                f"{path.relative_to(ROOT)}:{line_number}: duplicate summary row `{kind}`/`{name}`"
            )
            continue
        rows[key] = count
    return rows, findings


def matching_rows(file_path: str, rows: list[InventoryRow]) -> list[InventoryRow]:
    """Return inventory rows whose glob pattern covers a discovered file."""

    exact = [row for row in rows if row.pattern == file_path]
    if exact:
        return exact
    return [row for row in rows if fnmatchcase(file_path, row.pattern)]


def audit(
    rows: list[InventoryRow],
    files: list[str],
    targets: set[str],
    deleted_paths: set[str] | None = None,
) -> list[str]:
    """Return all audit findings for inventory rows and discovered files."""

    findings: list[str] = []
    deleted_paths = deleted_paths or set()
    matches_by_file = {
        file_path: matching_rows(file_path, rows) for file_path in files
    }
    files_by_row = {row: [] for row in rows}
    for file_path, matching in matches_by_file.items():
        for row in matching:
            files_by_row[row].append(file_path)
    for row in rows:
        location = f"{INVENTORY_PATH.relative_to(ROOT)}:{row.line}"
        if row.classification not in ALLOWED_CLASSIFICATIONS:
            findings.append(f"{location}: unknown classification `{row.classification}`")
        if row.classification in REQUIRES_REPLACEMENT_GATE and not row.replacement_gate:
            findings.append(f"{location}: classification `{row.classification}` requires a replacement gate")
        deleted_tombstone = row.pattern in deleted_paths
        if (
            row.replacement_gate
            and row.replacement_gate not in targets
            and not deleted_tombstone
        ):
            findings.append(f"{location}: replacement gate `{row.replacement_gate}` is not a Make target")
        if not row.owner:
            findings.append(f"{location}: owner is required")
        if not row.notes:
            findings.append(f"{location}: notes are required")
        matched = files_by_row[row]
        if not matched and not deleted_tombstone:
            findings.append(f"{location}: pattern `{row.pattern}` does not match any external test-suite file")
        if row.classification == "remove-non-portable" and matched:
            preview = ", ".join(matched[:5])
            if len(matched) > 5:
                preview += f", ... ({len(matched)} total)"
            findings.append(f"{location}: remove-non-portable files still exist: {preview}")

    for file_path in files:
        matches = matches_by_file[file_path]
        if not matches:
            findings.append(f"{file_path}: external VM test-suite file is not classified")
        elif len(matches) > 1:
            rendered = ", ".join(f"{row.pattern}@{row.line}" for row in matches)
            findings.append(f"{file_path}: external VM test-suite file matches multiple inventory rows: {rendered}")
    return findings


def audit_port_plan(
    plan_rows: list[PortPlanRow],
    inventory_rows: list[InventoryRow],
    files: list[str],
    targets: set[str],
    deleted_paths: set[str] | None = None,
) -> list[str]:
    """Return findings for the prioritized BEAM test port plan."""

    findings: list[str] = []
    deleted_paths = deleted_paths or set()
    inventory_patterns = {row.pattern for row in inventory_rows}
    seen_areas: dict[str, int] = {}
    for row in plan_rows:
        location = f"{PORT_PLAN_PATH.relative_to(ROOT)}:{row.line}"
        if row.priority not in ALLOWED_PRIORITIES:
            findings.append(f"{location}: unknown priority `{row.priority}`")
        if row.area in seen_areas:
            findings.append(f"{location}: duplicate port-plan area `{row.area}`")
        seen_areas[row.area] = row.line
        if row.classification not in ALLOWED_CLASSIFICATIONS:
            findings.append(f"{location}: unknown classification `{row.classification}`")
        if row.replacement_gate not in targets:
            findings.append(f"{location}: replacement gate `{row.replacement_gate}` is not a Make target")
        if not row.source_patterns:
            findings.append(f"{location}: at least one source pattern is required")
        if not row.first_port_action:
            findings.append(f"{location}: first port action is required")
        if not row.delete_rule:
            findings.append(f"{location}: delete rule is required")
        for pattern in row.source_patterns:
            if pattern not in inventory_patterns:
                findings.append(
                    f"{location}: source pattern `{pattern}` is not present in the suite inventory"
                )
                continue
            if not any(fnmatchcase(file_path, pattern) for file_path in files):
                if any(fnmatchcase(file_path, pattern) for file_path in deleted_paths):
                    continue
                findings.append(
                    f"{location}: source pattern `{pattern}` does not match discovered files"
                )
    missing_areas = sorted(REQUIRED_PORT_AREAS - set(seen_areas))
    if missing_areas:
        findings.append(
            f"{PORT_PLAN_PATH.relative_to(ROOT)}: missing required port-plan area(s): "
            f"{', '.join(missing_areas)}"
        )
    unknown_areas = sorted(set(seen_areas) - REQUIRED_PORT_AREAS)
    if unknown_areas:
        findings.append(
            f"{PORT_PLAN_PATH.relative_to(ROOT)}: unknown port-plan area(s): "
            f"{', '.join(unknown_areas)}"
        )
    return findings


def expected_summary(rows: list[InventoryRow], files: list[str]) -> dict[tuple[str, str], int]:
    """Return expected checked summary counts for discovered external tests."""

    summary: dict[tuple[str, str], int] = {("total", "files"): len(files)}
    for file_path in files:
        row = matching_rows(file_path, rows)[0]
        classification_key = ("classification", row.classification)
        owner_key = ("owner", row.owner)
        summary[classification_key] = summary.get(classification_key, 0) + 1
        summary[owner_key] = summary.get(owner_key, 0) + 1
    return summary


def audit_summary(
    checked_summary: dict[tuple[str, str], int],
    rows: list[InventoryRow],
    files: list[str],
) -> list[str]:
    """Return findings when checked summary counts drift from discovery."""

    findings: list[str] = []
    expected = expected_summary(rows, files)
    for key, expected_count in sorted(expected.items()):
        if key not in checked_summary:
            findings.append(
                f"{SUMMARY_PATH.relative_to(ROOT)}: missing summary row `{key[0]}`/`{key[1]}`"
            )
        elif checked_summary[key] != expected_count:
            findings.append(
                f"{SUMMARY_PATH.relative_to(ROOT)}: `{key[0]}`/`{key[1]}` expected {expected_count}, found {checked_summary[key]}"
            )
    for key in sorted(set(checked_summary) - set(expected)):
        findings.append(
            f"{SUMMARY_PATH.relative_to(ROOT)}: stale summary row `{key[0]}`/`{key[1]}`"
        )
    return findings


def expected_port_plan_summary(plan_rows: list[PortPlanRow]) -> dict[tuple[str, str], int]:
    """Return expected checked summary counts for the prioritized port plan."""

    summary: dict[tuple[str, str], int] = {("total", "areas"): len(plan_rows)}
    for row in plan_rows:
        priority_key = ("priority", row.priority)
        classification_key = ("classification", row.classification)
        gate_key = ("replacement_gate", row.replacement_gate)
        summary[priority_key] = summary.get(priority_key, 0) + 1
        summary[classification_key] = summary.get(classification_key, 0) + 1
        summary[gate_key] = summary.get(gate_key, 0) + 1
    return summary


def audit_port_plan_summary(
    checked_summary: dict[tuple[str, str], int],
    plan_rows: list[PortPlanRow],
) -> list[str]:
    """Return findings when checked port-plan counts drift from the plan."""

    findings: list[str] = []
    expected = expected_port_plan_summary(plan_rows)
    for key, expected_count in sorted(expected.items()):
        if key not in checked_summary:
            findings.append(
                f"{PORT_PLAN_SUMMARY_PATH.relative_to(ROOT)}: missing summary row `{key[0]}`/`{key[1]}`"
            )
        elif checked_summary[key] != expected_count:
            findings.append(
                f"{PORT_PLAN_SUMMARY_PATH.relative_to(ROOT)}: `{key[0]}`/`{key[1]}` expected {expected_count}, found {checked_summary[key]}"
            )
    for key in sorted(set(checked_summary) - set(expected)):
        findings.append(
            f"{PORT_PLAN_SUMMARY_PATH.relative_to(ROOT)}: stale summary row `{key[0]}`/`{key[1]}`"
        )
    return findings


def audit_checked_summary(
    path: Path,
    checked_summary: dict[tuple[str, str], int],
    expected: dict[tuple[str, str], int],
) -> list[str]:
    """Return findings when a generic checked count summary drifts."""

    findings: list[str] = []
    for key, expected_count in sorted(expected.items()):
        if key not in checked_summary:
            findings.append(
                f"{path.relative_to(ROOT)}: missing summary row `{key[0]}`/`{key[1]}`"
            )
        elif checked_summary[key] != expected_count:
            findings.append(
                f"{path.relative_to(ROOT)}: `{key[0]}`/`{key[1]}` "
                f"expected {expected_count}, found {checked_summary[key]}"
            )
    for key in sorted(set(checked_summary) - set(expected)):
        findings.append(
            f"{path.relative_to(ROOT)}: stale summary row `{key[0]}`/`{key[1]}`"
        )
    return findings


def audit_port_status(
    status_rows: list[PortStatusRow],
    plan_rows: list[PortPlanRow],
    files: list[str],
    targets: set[str],
) -> list[str]:
    """Return findings for the executable port/deletion checklist."""

    findings: list[str] = []
    plan_by_area = {row.area: row for row in plan_rows}
    seen_areas: dict[str, int] = {}
    for row in status_rows:
        location = f"{PORT_STATUS_PATH.relative_to(ROOT)}:{row.line}"
        if row.area in seen_areas:
            findings.append(f"{location}: duplicate port-status area `{row.area}`")
        seen_areas[row.area] = row.line
        plan_row = plan_by_area.get(row.area)
        if plan_row is None:
            findings.append(f"{location}: area `{row.area}` is not present in the port plan")
        else:
            if row.priority != plan_row.priority:
                findings.append(
                    f"{location}: priority `{row.priority}` does not match port plan `{plan_row.priority}`"
                )
            if row.classification != plan_row.classification:
                findings.append(
                    f"{location}: classification `{row.classification}` does not match port plan `{plan_row.classification}`"
                )
            if row.replacement_gate != plan_row.replacement_gate:
                findings.append(
                    f"{location}: replacement gate `{row.replacement_gate}` does not match port plan `{plan_row.replacement_gate}`"
                )
        if row.replacement_gate and row.replacement_gate not in targets:
            findings.append(f"{location}: replacement gate `{row.replacement_gate}` is not a Make target")
        if row.execution_path not in ALLOWED_EXECUTION_PATHS:
            findings.append(f"{location}: unknown execution path `{row.execution_path}`")
        if row.execution_path == "not-proven":
            if row.execution_gate != "-":
                findings.append(
                    f"{location}: not-proven execution path must use `-` execution gate"
                )
        else:
            if row.execution_gate in {"", "-"}:
                findings.append(
                    f"{location}: execution path `{row.execution_path}` requires an execution gate"
                )
            elif row.execution_gate not in targets:
                findings.append(
                    f"{location}: execution gate `{row.execution_gate}` is not a Make target"
                )
        if (
            row.execution_path == "native-aot"
            and row.execution_gate not in NATIVE_AOT_PROOF_GATES
        ):
            findings.append(
                f"{location}: native-aot execution requires a recognized AOT proof gate"
            )
        if row.port_status not in ALLOWED_PORT_STATUSES:
            findings.append(f"{location}: unknown port status `{row.port_status}`")
        if row.deletion_status not in ALLOWED_DELETION_STATUSES:
            findings.append(f"{location}: unknown deletion status `{row.deletion_status}`")
        if not row.notes:
            findings.append(f"{location}: notes are required")
        if row.port_status == "ported" and row.deletion_status != "deleted":
            findings.append(f"{location}: fully ported area must also mark legacy suite deletion complete")
        if row.port_status == "ported" and row.execution_path == "not-proven":
            findings.append(f"{location}: fully ported area requires a proven execution path")
        if row.port_status == "not-ported" and row.execution_path != "not-proven":
            findings.append(f"{location}: not-ported area cannot claim a proven execution path")
        if row.deletion_status == "deleted" and plan_row is not None:
            remaining = [
                file_path
                for file_path in files
                if any(fnmatchcase(file_path, pattern) for pattern in plan_row.source_patterns)
            ]
            if remaining:
                preview = ", ".join(remaining[:5])
                if len(remaining) > 5:
                    preview += f", ... ({len(remaining)} total)"
                findings.append(f"{location}: deletion is checked but legacy files still exist: {preview}")
    missing_areas = sorted(set(plan_by_area) - set(seen_areas))
    if missing_areas:
        findings.append(
            f"{PORT_STATUS_PATH.relative_to(ROOT)}: missing port-status area(s): "
            f"{', '.join(missing_areas)}"
        )
    stale_areas = sorted(set(seen_areas) - set(plan_by_area))
    if stale_areas:
        findings.append(
            f"{PORT_STATUS_PATH.relative_to(ROOT)}: stale port-status area(s): "
            f"{', '.join(stale_areas)}"
        )
    return findings


def print_summary(
    rows: list[InventoryRow],
    files: list[str],
    plan_rows: list[PortPlanRow],
    status_rows: list[PortStatusRow],
    file_status_rows: list[FileStatusRow],
    deletion_rows: list[DeletionManifestRow],
) -> None:
    """Print stable audit counts for successful runs."""

    by_classification: dict[str, int] = {}
    by_owner: dict[str, int] = {}
    for file_path in files:
        row = matching_rows(file_path, rows)[0]
        by_classification[row.classification] = by_classification.get(row.classification, 0) + 1
        by_owner[row.owner] = by_owner.get(row.owner, 0) + 1

    print(f"terlan-vm BEAM test-suite audit: {len(files)} files classified")
    print("by classification:")
    for classification, count in sorted(by_classification.items()):
        print(f"  {classification}: {count}")
    print("by owner:")
    for owner, count in sorted(by_owner.items()):
        print(f"  {owner}: {count}")
    print(f"port plan areas: {len(plan_rows)}")
    by_priority: dict[str, int] = {}
    for row in plan_rows:
        by_priority[row.priority] = by_priority.get(row.priority, 0) + 1
    print("by port-plan priority:")
    for priority, count in sorted(by_priority.items()):
        print(f"  {priority}: {count}")
    by_port_status: dict[str, int] = {}
    by_deletion_status: dict[str, int] = {}
    by_execution_path: dict[str, int] = {}
    for row in status_rows:
        by_port_status[row.port_status] = by_port_status.get(row.port_status, 0) + 1
        by_deletion_status[row.deletion_status] = by_deletion_status.get(row.deletion_status, 0) + 1
        by_execution_path[row.execution_path] = by_execution_path.get(row.execution_path, 0) + 1
    print("by port status:")
    for status, count in sorted(by_port_status.items()):
        print(f"  {status}: {count}")
    print("by deletion status:")
    for status, count in sorted(by_deletion_status.items()):
        print(f"  {status}: {count}")
    print("by execution path:")
    for execution_path, count in sorted(by_execution_path.items()):
        print(f"  {execution_path}: {count}")
    file_summary = expected_file_status_summary(file_status_rows)
    print("active file-level migration progress:")
    print(f"  ported: {file_summary.get(('port_status', 'ported'), 0)}")
    print(f"  not-ported: {file_summary.get(('port_status', 'not-ported'), 0)}")
    print(f"completed deletion manifest: {len(deletion_rows)}")
    by_deletion_classification: dict[str, int] = {}
    for row in deletion_rows:
        by_deletion_classification[row.classification] = (
            by_deletion_classification.get(row.classification, 0) + 1
        )
    for classification, count in sorted(by_deletion_classification.items()):
        print(f"  {classification}: {count}")


def main() -> int:
    """Run the external BEAM test-suite parity audit."""

    root = vm_root()
    if not root.is_dir():
        if "TERLAN_VM_ROOT" in os.environ:
            print(f"terlan-vm BEAM test-suite audit failed: TERLAN_VM_ROOT does not exist: {root}", file=sys.stderr)
            return 1
        print(
            "terlan-vm BEAM test-suite audit skipped: no sibling terlan-vm checkout found; "
            "set TERLAN_VM_ROOT to require a specific checkout.",
            file=sys.stderr,
        )
        return 0

    rows, findings = read_inventory(INVENTORY_PATH)
    plan_rows, plan_findings = read_port_plan(PORT_PLAN_PATH)
    status_rows, status_findings = read_port_status(PORT_STATUS_PATH)
    file_status_rows, file_status_findings = read_file_status(FILE_STATUS_PATH, ROOT)
    deletion_rows, deletion_findings = read_deletion_manifest(
        DELETION_MANIFEST_PATH, ROOT
    )
    file_status_summary, file_status_summary_findings = read_summary(
        FILE_STATUS_SUMMARY_PATH
    )
    plan_summary_rows, plan_summary_findings = read_summary(PORT_PLAN_SUMMARY_PATH)
    summary_rows, summary_findings = read_summary(SUMMARY_PATH)
    findings.extend(plan_findings)
    findings.extend(status_findings)
    findings.extend(file_status_findings)
    findings.extend(deletion_findings)
    findings.extend(file_status_summary_findings)
    findings.extend(plan_summary_findings)
    findings.extend(summary_findings)
    files = discover_test_suite_files(root)
    external_makefile = root / "GNUmakefile"
    if external_makefile.is_file():
        findings.extend(
            audit_external_make_test_targets(
                external_makefile.read_text(encoding="utf-8"),
                set(files),
            )
        )
    else:
        findings.append("terlan-vm/GNUmakefile: missing external VM Make harness")
    targets = make_targets_from_paths([MAKEFILE_PATH])
    deleted_paths = {row.path for row in deletion_rows}
    findings.extend(audit(rows, files, targets, deleted_paths))
    findings.extend(audit_port_plan(plan_rows, rows, files, targets, deleted_paths))
    findings.extend(audit_port_status(status_rows, plan_rows, files, targets))
    active_paths = {row.path for row in file_status_rows}
    tracked_paths = set(files) | active_paths | deleted_paths
    expected_gates: dict[str, str] = {}
    expected_classifications: dict[str, str] = {}
    for source_path in tracked_paths:
        matches = matching_rows(source_path, rows)
        if len(matches) == 1:
            expected_gates[source_path] = matches[0].replacement_gate
            expected_classifications[source_path] = matches[0].classification
    findings.extend(
        audit_file_status(
            file_status_rows,
            files,
            expected_gates,
            targets,
            FILE_STATUS_PATH,
            ROOT,
        )
    )
    findings.extend(
        audit_deletion_manifest(
            deletion_rows,
            files,
            active_paths,
            expected_classifications,
            expected_gates,
            targets,
            DELETION_MANIFEST_PATH,
            ROOT,
        )
    )
    findings.extend(
        audit_checked_summary(
            FILE_STATUS_SUMMARY_PATH,
            file_status_summary,
            expected_file_status_summary(file_status_rows),
        )
    )
    findings.extend(audit_port_plan_summary(plan_summary_rows, plan_rows))
    findings.extend(audit_summary(summary_rows, rows, files))
    if findings:
        print("terlan-vm BEAM test-suite audit failed:", file=sys.stderr)
        for finding in findings:
            print(f"  - {finding}", file=sys.stderr)
        return 1
    print_summary(
        rows,
        files,
        plan_rows,
        status_rows,
        file_status_rows,
        deletion_rows,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
