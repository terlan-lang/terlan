"""File-level migration status for the external BEAM/OTP test corpus."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


ALLOWED_PORT_STATUSES = {"not-ported", "ported"}
ALLOWED_DELETION_CLASSIFICATIONS = {
    "port-to-rust-vm-test",
    "port-to-terlan-test",
    "delete-after-vm-equivalent",
    "remove-non-portable",
}


@dataclass(frozen=True)
class FileStatusRow:
    """Active migration state and replacement evidence for one external file."""

    path: str
    port_status: str
    replacement_evidence: str
    line: int


@dataclass(frozen=True)
class DeletionManifestRow:
    """Compact tombstone for one completed external test-file removal."""

    path: str
    classification: str
    replacement_gate: str
    corpus_generation: str
    line: int


def read_file_status(
    path: Path, root: Path
) -> tuple[list[FileStatusRow], list[str]]:
    """Read the checked file-level status ledger."""

    rows: list[FileStatusRow] = []
    findings: list[str] = []
    if not path.is_file():
        return rows, [f"{path.relative_to(root)}: missing file-level status ledger"]
    for line_number, raw_line in enumerate(
        path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        parts = raw_line.split("\t")
        if len(parts) != 3:
            findings.append(
                f"{path.relative_to(root)}:{line_number}: expected 3 TSV fields, "
                f"found {len(parts)}"
            )
            continue
        source_path, port_status, evidence = parts
        rows.append(
            FileStatusRow(
                path=source_path,
                port_status=port_status,
                replacement_evidence=evidence,
                line=line_number,
            )
        )
    return rows, findings


def read_deletion_manifest(
    path: Path, root: Path
) -> tuple[list[DeletionManifestRow], list[str]]:
    """Read compact completed-deletion tombstones plus diagnostics."""

    rows: list[DeletionManifestRow] = []
    findings: list[str] = []
    if not path.is_file():
        return rows, [f"{path.relative_to(root)}: missing deletion manifest"]
    for line_number, raw_line in enumerate(
        path.read_text(encoding="utf-8").splitlines(), start=1
    ):
        line = raw_line.strip()
        if not line or line.startswith("#"):
            continue
        parts = raw_line.split("\t")
        if len(parts) != 4:
            findings.append(
                f"{path.relative_to(root)}:{line_number}: expected 4 TSV fields, "
                f"found {len(parts)}"
            )
            continue
        source_path, classification, replacement_gate, corpus_generation = parts
        rows.append(
            DeletionManifestRow(
                path=source_path,
                classification=classification,
                replacement_gate=replacement_gate,
                corpus_generation=corpus_generation,
                line=line_number,
            )
        )
    return rows, findings


def audit_file_status(
    rows: list[FileStatusRow],
    discovered_files: list[str],
    expected_gates: dict[str, str],
    make_targets: set[str],
    status_path: Path,
    root: Path,
) -> list[str]:
    """Validate exact active coverage and replacement evidence."""

    findings: list[str] = []
    discovered = set(discovered_files)
    seen: dict[str, int] = {}
    ledger_paths = [row.path for row in rows]
    if ledger_paths != sorted(ledger_paths):
        findings.append(f"{status_path.relative_to(root)}: file status rows must be sorted by path")

    for row in rows:
        location = f"{status_path.relative_to(root)}:{row.line}"
        if row.path in seen:
            findings.append(
                f"{location}: duplicate file status for `{row.path}`; "
                f"first declared on line {seen[row.path]}"
            )
        seen[row.path] = row.line
        if row.port_status not in ALLOWED_PORT_STATUSES:
            findings.append(f"{location}: unknown port status `{row.port_status}`")
        expected_gate = expected_gates.get(row.path)
        if expected_gate is None:
            findings.append(f"{location}: path is not covered by exactly one inventory row")
        if row.port_status == "ported":
            if row.replacement_evidence in {"", "-"}:
                findings.append(f"{location}: ported file requires replacement evidence")
            elif row.replacement_evidence != expected_gate:
                findings.append(
                    f"{location}: replacement evidence `{row.replacement_evidence}` "
                    f"does not match inventory gate `{expected_gate}`"
                )
            elif row.replacement_evidence not in make_targets:
                findings.append(
                    f"{location}: replacement evidence `{row.replacement_evidence}` "
                    "is not a Make target"
                )
        elif row.replacement_evidence != "-":
            findings.append(
                f"{location}: not-ported file must use `-` replacement evidence"
            )

        if row.path not in discovered:
            findings.append(f"{location}: active file is absent from the external corpus")

    for source_path in sorted(discovered - set(seen)):
        findings.append(f"{source_path}: missing file-level migration status")
    return findings


def audit_deletion_manifest(
    rows: list[DeletionManifestRow],
    discovered_files: list[str],
    active_paths: set[str],
    expected_classifications: dict[str, str],
    expected_gates: dict[str, str],
    make_targets: set[str],
    manifest_path: Path,
    root: Path,
) -> list[str]:
    """Validate compact tombstones and reject deleted-file reintroduction."""

    findings: list[str] = []
    discovered = set(discovered_files)
    seen: dict[str, int] = {}
    manifest_paths = [row.path for row in rows]
    if manifest_paths != sorted(manifest_paths):
        findings.append(
            f"{manifest_path.relative_to(root)}: deletion rows must be sorted by path"
        )

    for row in rows:
        location = f"{manifest_path.relative_to(root)}:{row.line}"
        if row.path in seen:
            findings.append(
                f"{location}: duplicate deletion tombstone for `{row.path}`; "
                f"first declared on line {seen[row.path]}"
            )
        seen[row.path] = row.line
        if row.path in active_paths:
            findings.append(f"{location}: deleted path also appears in the active ledger")
        if row.path in discovered:
            findings.append(f"{location}: deleted external test file has been reintroduced")
        if row.classification not in ALLOWED_DELETION_CLASSIFICATIONS:
            findings.append(
                f"{location}: unknown deletion classification `{row.classification}`"
            )
        expected_classification = expected_classifications.get(row.path)
        if expected_classification is None:
            findings.append(f"{location}: path is not covered by exactly one inventory row")
        elif row.classification != expected_classification:
            findings.append(
                f"{location}: classification `{row.classification}` does not match "
                f"inventory `{expected_classification}`"
            )
        expected_gate = expected_gates.get(row.path, "") or "-"
        if row.replacement_gate != expected_gate:
            findings.append(
                f"{location}: replacement gate `{row.replacement_gate}` does not match "
                f"inventory `{expected_gate}`"
            )
        elif row.replacement_gate != "-" and row.replacement_gate not in make_targets:
            findings.append(
                f"{location}: replacement gate `{row.replacement_gate}` is not a Make target"
            )
        if not row.corpus_generation:
            findings.append(f"{location}: corpus generation is required")
    return findings


def expected_file_status_summary(
    rows: list[FileStatusRow],
) -> dict[tuple[str, str], int]:
    """Return stable file-level progress counts for checked summary validation."""

    summary: dict[tuple[str, str], int] = {
        ("total", "active-files"): len(rows),
        ("port_status", "not-ported"): 0,
        ("port_status", "ported"): 0,
    }
    for row in rows:
        port_key = ("port_status", row.port_status)
        summary[port_key] = summary.get(port_key, 0) + 1
    return summary
