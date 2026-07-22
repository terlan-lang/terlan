#!/usr/bin/env python3
"""Validate VM benchmark family planning artifacts.

Inputs:
- `docs/runtime/VM_BENCHMARK_FAMILIES.tsv`.
- One expected Make gate name passed on the command line.
- The clue report files referenced by the selected benchmark family.

Outputs:
- Exit status 0 when the selected benchmark family is present, has the
  required tracks for its comparison domain, and links a clue report with the
  required code-analysis columns.
- Exit status 1 with stable diagnostics for missing rows, malformed TSV
  fields, missing required tracks, missing clue reports, or incomplete clue
  report tables.

Transformation:
- Keeps the 0.0.7 benchmark roadmap executable before expensive benchmark
  implementations land, while requiring future slower lanes to produce
  source-level performance clues instead of raw numbers alone.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import sys


ROOT = Path(__file__).resolve().parents[1]
DOC_ROOT_CANDIDATES = (
    ROOT / "docs",
    ROOT.parent / "docs",
)
MANIFEST_RELATIVE = Path("runtime") / "VM_BENCHMARK_FAMILIES.tsv"
PORT_PLAN_RELATIVE = Path("runtime") / "TERLAN_VM_BEAM_TEST_PORT_PLAN.tsv"
SEMANTICS_EQUIVALENCE_RELATIVE = Path("runtime") / "VM_SEMANTICS_EQUIVALENCE_MATRIX.tsv"
MAKEFILE_RELATIVE = Path("Makefile")

EXPECTED_TRACKS = {
    "vm-http-vs-axum-check": {
        "static",
        "json-handler",
        "metadata",
        "route-matching",
        "keep-alive",
        "concurrency-1",
        "concurrency-100",
        "concurrency-1000",
    },
    "vm-semantics-vs-otp-check": {
        "process-spawn",
        "mailbox-send-receive",
        "selective-receive",
        "timers",
        "links-monitors",
        "supervision",
        "registry",
        "scheduler-fairness",
        "hot-reload",
        "distribution",
    },
}

EXPECTED_STATUS = {
    "vm-http-vs-axum-check": "executable-baseline-with-socket-permission-skip",
    "vm-semantics-vs-otp-check": "planned-after-terlan-vm-erl-suite-audit",
}

REQUIRED_CLUE_COLUMNS = {
    "Lane",
    "Symptom",
    "Suspected Subsystem",
    "Source Files",
    "Hypothesis",
    "Next Measurement",
}

SEMANTICS_REQUIRED_CLUE_COLUMNS = REQUIRED_CLUE_COLUMNS | {"Port Plan Area"}

SEMANTICS_TRACK_PORT_AREAS = {
    "process-spawn": "process-registry",
    "mailbox-send-receive": "mailbox",
    "selective-receive": "mailbox",
    "timers": "timers",
    "links-monitors": "links-monitors",
    "supervision": "links-monitors",
    "registry": "process-registry",
    "scheduler-fairness": "scheduler",
    "hot-reload": "scheduler",
    "distribution": "distribution-framing",
}

ALLOWED_EQUIVALENCE_STATUS = {"planned", "partial", "complete"}


@dataclass(frozen=True)
class BenchmarkFamily:
    """One benchmark family manifest row."""

    family: str
    gate: str
    reference: str
    scope: str
    required_tracks: set[str]
    clue_report: Path
    status: str


@dataclass(frozen=True)
class SemanticsEquivalenceRow:
    """One executable VM-vs-OTP semantic equivalence planning row."""

    track: str
    port_plan_area: str
    vm_gate: str
    otp_reference: str
    status: str
    current_evidence: str
    line: int


def docs_root() -> Path:
    """Return the docs root that contains the benchmark manifest."""

    for candidate in DOC_ROOT_CANDIDATES:
        if (candidate / MANIFEST_RELATIVE).is_file():
            return candidate
    return DOC_ROOT_CANDIDATES[0]


def port_plan_path() -> Path:
    """Return the available BEAM test port-plan path."""

    for candidate in DOC_ROOT_CANDIDATES:
        path = candidate / PORT_PLAN_RELATIVE
        if path.is_file():
            return path
    return DOC_ROOT_CANDIDATES[0] / PORT_PLAN_RELATIVE


def semantics_equivalence_path(root: Path) -> Path:
    """Return the semantics equivalence matrix path for the selected docs root."""

    return root / SEMANTICS_EQUIVALENCE_RELATIVE


def makefile_path() -> Path:
    """Return the golden repo Makefile path used for VM gate validation."""

    return ROOT / MAKEFILE_RELATIVE


def read_manifest(path: Path) -> tuple[list[BenchmarkFamily], list[str]]:
    """Parse the benchmark-family manifest."""

    diagnostics: list[str] = []
    rows: list[BenchmarkFamily] = []
    if not path.is_file():
        return rows, [f"{path}: missing benchmark family manifest"]
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not raw_line.strip() or raw_line.startswith("#"):
            continue
        parts = raw_line.split("\t")
        if len(parts) != 7:
            diagnostics.append(f"{path}:{line_number}: expected 7 TSV fields, found {len(parts)}")
            continue
        family, gate, reference, scope, tracks, clue_report, status = parts
        rows.append(
            BenchmarkFamily(
                family=family,
                gate=gate,
                reference=reference,
                scope=scope,
                required_tracks={track for track in tracks.split(",") if track},
                clue_report=Path(clue_report),
                status=status,
            )
        )
    return rows, diagnostics


def read_semantics_equivalence_matrix(
    path: Path,
) -> tuple[list[SemanticsEquivalenceRow], list[str]]:
    """Parse the VM-vs-OTP semantics equivalence matrix."""

    diagnostics: list[str] = []
    rows: list[SemanticsEquivalenceRow] = []
    if not path.is_file():
        return rows, [f"{path}: missing semantics equivalence matrix"]
    for line_number, raw_line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not raw_line.strip() or raw_line.startswith("#"):
            continue
        parts = raw_line.split("\t")
        if len(parts) != 6:
            diagnostics.append(f"{path}:{line_number}: expected 6 TSV fields, found {len(parts)}")
            continue
        track, port_plan_area, vm_gate, otp_reference, status, current_evidence = parts
        rows.append(
            SemanticsEquivalenceRow(
                track=track,
                port_plan_area=port_plan_area,
                vm_gate=vm_gate,
                otp_reference=otp_reference,
                status=status,
                current_evidence=current_evidence,
                line=line_number,
            )
        )
    return rows, diagnostics


def validate_family(root: Path, family: BenchmarkFamily) -> list[str]:
    """Validate one benchmark family row."""

    diagnostics: list[str] = []
    expected = EXPECTED_TRACKS.get(family.gate)
    if expected is None:
        diagnostics.append(f"{family.gate}: unknown benchmark family gate")
        return diagnostics
    missing = sorted(expected - family.required_tracks)
    if missing:
        diagnostics.append(f"{family.gate}: missing required track(s): {', '.join(missing)}")
    if family.reference not in {"axum-tokio", "otp"}:
        diagnostics.append(f"{family.gate}: unsupported reference `{family.reference}`")
    expected_status = EXPECTED_STATUS[family.gate]
    if family.status != expected_status:
        diagnostics.append(f"{family.gate}: status must be {expected_status}")
    report_path = root.parent / family.clue_report
    diagnostics.extend(validate_clue_report(family.gate, report_path))
    if family.gate == "vm-semantics-vs-otp-check":
        diagnostics.extend(validate_semantics_port_plan(family))
        matrix_path = semantics_equivalence_path(root)
        rows, matrix_diagnostics = read_semantics_equivalence_matrix(matrix_path)
        diagnostics.extend(matrix_diagnostics)
        diagnostics.extend(validate_semantics_equivalence_matrix(family, matrix_path, rows))
    return diagnostics


def validate_clue_report(gate: str, path: Path) -> list[str]:
    """Validate the clue report linked by one benchmark family."""

    if not path.is_file():
        return [f"{gate}: missing clue report {path}"]
    text = path.read_text(encoding="utf-8")
    required_columns = (
        SEMANTICS_REQUIRED_CLUE_COLUMNS
        if gate == "vm-semantics-vs-otp-check"
        else REQUIRED_CLUE_COLUMNS
    )
    missing = [column for column in sorted(required_columns) if column not in text]
    if missing:
        return [f"{gate}: clue report {path} missing column(s): {', '.join(missing)}"]
    if gate not in text:
        return [f"{gate}: clue report {path} does not name the gate"]
    return validate_clue_report_source_files(gate, path, text)


def validate_clue_report_source_files(gate: str, path: Path, text: str) -> list[str]:
    """Validate that clue report source-file cells point at real files."""

    diagnostics: list[str] = []
    rows = markdown_table_rows(text)
    for header, cells in rows:
        if "Source Files" not in header:
            continue
        source_index = header.index("Source Files")
        if source_index >= len(cells):
            diagnostics.append(f"{gate}: clue row in {path} is missing Source Files cell")
            continue
        for source in source_file_cells(cells[source_index]):
            if not source_exists(source):
                diagnostics.append(f"{gate}: clue report {path} references missing source file {source}")
    return diagnostics


def markdown_table_rows(text: str) -> list[tuple[list[str], list[str]]]:
    """Return Markdown table rows paired with their header cells."""

    rows: list[tuple[list[str], list[str]]] = []
    header: list[str] | None = None
    for raw_line in text.splitlines():
        line = raw_line.strip()
        if not line.startswith("|") or not line.endswith("|"):
            header = None
            continue
        cells = [cell.strip() for cell in line.strip("|").split("|")]
        if all(set(cell) <= {"-", ":", " "} for cell in cells):
            continue
        if header is None:
            header = cells
            continue
        rows.append((header, cells))
    return rows


def source_file_cells(cell: str) -> list[str]:
    """Split one Source Files Markdown cell into normalized source paths."""

    sources: list[str] = []
    for raw_source in cell.split(","):
        source = raw_source.strip().strip("`")
        if not source or source == "pending":
            continue
        source = source.split("::", 1)[0]
        sources.append(source)
    return sources


def source_exists(source: str) -> bool:
    """Return whether a clue source path exists in the repo or workspace."""

    return (ROOT / source).is_file() or (ROOT.parent / source).is_file()


def validate_semantics_port_plan(family: BenchmarkFamily) -> list[str]:
    """Validate that VM semantics tracks map to the BEAM test port plan."""

    diagnostics: list[str] = []
    path = port_plan_path()
    if not path.is_file():
        return [f"{family.gate}: missing BEAM test port plan {path}"]
    text = path.read_text(encoding="utf-8")
    areas = {
        parts[1]
        for raw_line in text.splitlines()
        if raw_line.strip() and not raw_line.startswith("#")
        for parts in [raw_line.split("\t")]
        if len(parts) >= 2
    }
    for track in sorted(family.required_tracks):
        area = SEMANTICS_TRACK_PORT_AREAS.get(track)
        if area is None:
            diagnostics.append(f"{family.gate}: track `{track}` has no port-plan area mapping")
            continue
        if area not in areas:
            diagnostics.append(
                f"{family.gate}: track `{track}` maps to missing port-plan area `{area}`"
            )
    return diagnostics


def validate_semantics_equivalence_matrix(
    family: BenchmarkFamily,
    path: Path,
    rows: list[SemanticsEquivalenceRow],
) -> list[str]:
    """Validate the VM-vs-OTP equivalence matrix for required semantics tracks."""

    diagnostics: list[str] = []
    expected_tracks = EXPECTED_TRACKS[family.gate]
    make_targets = read_make_targets(makefile_path())
    port_plan_areas = read_port_plan_areas(port_plan_path())
    seen_tracks: dict[str, int] = {}
    for row in rows:
        location = f"{path}:{row.line}"
        if row.track in seen_tracks:
            diagnostics.append(f"{location}: duplicate equivalence track `{row.track}`")
        seen_tracks[row.track] = row.line
        if row.track not in expected_tracks:
            diagnostics.append(f"{location}: unknown equivalence track `{row.track}`")
        expected_area = SEMANTICS_TRACK_PORT_AREAS.get(row.track)
        if expected_area is not None and row.port_plan_area != expected_area:
            diagnostics.append(
                f"{location}: track `{row.track}` must map to port-plan area `{expected_area}`, found `{row.port_plan_area}`"
            )
        if row.port_plan_area not in port_plan_areas:
            diagnostics.append(f"{location}: unknown port-plan area `{row.port_plan_area}`")
        if row.vm_gate not in make_targets:
            diagnostics.append(f"{location}: VM gate `{row.vm_gate}` is not a Make target")
        if not row.otp_reference:
            diagnostics.append(f"{location}: OTP reference description is required")
        if row.status not in ALLOWED_EQUIVALENCE_STATUS:
            diagnostics.append(f"{location}: unknown equivalence status `{row.status}`")
        if row.status in {"partial", "complete"} and not row.current_evidence:
            diagnostics.append(f"{location}: current evidence is required for `{row.status}` status")
        if row.status == "complete" and "benchmark" not in row.current_evidence.lower():
            diagnostics.append(
                f"{location}: complete equivalence status must name benchmark evidence"
            )

    missing_tracks = sorted(expected_tracks - set(seen_tracks))
    if missing_tracks:
        diagnostics.append(
            f"{path}: missing equivalence track(s): {', '.join(missing_tracks)}"
        )
    stale_tracks = sorted(set(seen_tracks) - expected_tracks)
    if stale_tracks:
        diagnostics.append(
            f"{path}: stale equivalence track(s): {', '.join(stale_tracks)}"
        )
    return diagnostics


def read_make_targets(path: Path) -> set[str]:
    """Return Make targets from the golden repo root Makefile and included editor file."""

    if not path.is_file():
        return set()
    text = path.read_text(encoding="utf-8")
    for include in text.splitlines():
        include_path = include.strip().removeprefix("include ").strip()
        if include_path == include.strip():
            continue
        included = path.parent / include_path
        if included.is_file():
            text += "\n" + included.read_text(encoding="utf-8")
    targets: set[str] = set()
    for raw_line in text.splitlines():
        if raw_line.startswith("\t") or raw_line.startswith("."):
            continue
        if ":" not in raw_line:
            continue
        target = raw_line.split(":", 1)[0].strip()
        if target and " " not in target and "\t" not in target:
            targets.add(target)
    return targets


def read_port_plan_areas(path: Path) -> set[str]:
    """Return known BEAM test port-plan areas."""

    if not path.is_file():
        return set()
    areas: set[str] = set()
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        if not raw_line.strip() or raw_line.startswith("#"):
            continue
        parts = raw_line.split("\t")
        if len(parts) >= 2:
            areas.add(parts[1])
    return areas


def main(argv: list[str]) -> int:
    """Validate the benchmark family selected by `argv`."""

    if len(argv) != 2:
        print("usage: check_vm_benchmark_family_plan.py <make-gate>", file=sys.stderr)
        return 1
    gate = argv[1]
    if gate not in EXPECTED_TRACKS:
        print(f"{gate}: unsupported benchmark gate", file=sys.stderr)
        return 1
    root = docs_root()
    manifest = root / MANIFEST_RELATIVE
    rows, diagnostics = read_manifest(manifest)
    matches = [row for row in rows if row.gate == gate]
    if len(matches) != 1:
        diagnostics.append(f"{gate}: expected exactly one manifest row, found {len(matches)}")
    for row in matches:
        diagnostics.extend(validate_family(root, row))
    if diagnostics:
        for diagnostic in diagnostics:
            print(diagnostic, file=sys.stderr)
        return 1
    print(f"[{gate}] benchmark family plan validated")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
