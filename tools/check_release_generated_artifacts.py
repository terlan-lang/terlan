#!/usr/bin/env python3
"""Validate the deterministic release-generated-artifact inventory."""

from __future__ import annotations

import argparse
import copy
import glob
import hashlib
import json
from pathlib import Path, PurePosixPath
import re
import tempfile
from typing import Any

from makefile_contract import make_target_body, make_targets_from_paths


INVENTORY_PATH = Path("docs/release/GENERATED_ARTIFACTS.json")
REPORT_PATH = Path("target/quality/release-generated-artifacts-report.json")
SCHEMA = "terlan.release-generated-artifacts.v1"
GATE_TARGET = "release-generated-artifacts-check"
FRESHNESS_TARGET = "release-generated-artifacts-freshness-pass"
CLASSIFICATIONS = {"committed", "packaged-only", "cache-only", "ignored-build-output"}
REQUIRED_FIELDS = {
    "id",
    "classification",
    "paths",
    "regenerate",
    "freshness_gate",
    "owner",
}
FORBIDDEN_TERMS = ("todo", "tbd", "placeholder", "fixme", "unknown")
HOST_PATH_PATTERNS = {
    "unix-home": re.compile(rb"(?<![A-Za-z0-9])/(?:home|Users)/[^\s\"'<>]+"),
    "unix-temp": re.compile(rb"(?<![A-Za-z0-9])/(?:private/)?tmp/[^\s\"'<>]+"),
    "windows-user": re.compile(
        rb"(?i)(?:[A-Z]:\\|\\\\[^\\\s]+\\)(?:Users|Documents and Settings)\\[^\s\"'<>]+"
    ),
}


def repository_make_targets(root: Path) -> set[str]:
    """Return targets from the root Makefile and included subsystem makefiles."""
    makefiles = [root / "Makefile", *sorted(root.glob("**/*.mk"))]
    return make_targets_from_paths(makefiles)


def matched_artifact_paths(root: Path, pattern: str) -> list[Path]:
    """Expand one repository-relative artifact pattern in stable path order."""
    return sorted(
        (Path(path) for path in glob.glob(str(root / pattern), recursive=True)),
        key=lambda path: path.as_posix(),
    )


def validate_aggregate_recipe(inventory: dict[str, Any], makefile_text: str) -> list[str]:
    """Require the umbrella gate to execute every inventoried freshness gate."""
    body = make_target_body(makefile_text, GATE_TARGET)
    if body is None:
        return [f"Makefile is missing aggregate target `{GATE_TARGET}`"]
    diagnostics: list[str] = []
    freshness_body = make_target_body(makefile_text, FRESHNESS_TARGET)
    if freshness_body is None:
        return [f"Makefile is missing freshness target `{FRESHNESS_TARGET}`"]
    gates = sorted({row["freshness_gate"] for row in inventory["artifacts"]})
    for gate in gates:
        command = f"$(MAKE) --no-print-directory {gate}"
        if command not in freshness_body:
            diagnostics.append(
                f"Makefile target `{FRESHNESS_TARGET}` must execute `{command}`"
            )
    freshness_command = f"$(MAKE) --no-print-directory {FRESHNESS_TARGET}"
    if body.count(freshness_command) != 2:
        diagnostics.append(
            f"Makefile target `{GATE_TARGET}` must execute `{freshness_command}` exactly twice"
        )
    for command in (
        "$(PYTHON) tools/check_release_generated_artifacts.py --record-snapshot target/quality/release-generated-artifacts-before.json",
        "$(PYTHON) tools/check_release_generated_artifacts.py --compare-snapshot target/quality/release-generated-artifacts-before.json",
        "$(PYTHON) tools/check_release_generated_artifacts.py --self-test",
        "$(PYTHON) tools/check_release_generated_artifacts.py --regeneration-run-count 2",
    ):
        required_count = 2 if "--compare-snapshot" in command else 1
        if body.count(command) != required_count:
            diagnostics.append(
                f"Makefile target `{GATE_TARGET}` must execute `{command}` exactly {required_count} time(s)"
            )
    return diagnostics


def validate_inventory(
    inventory: Any,
    root: Path,
    targets: set[str],
    *,
    require_paths: bool = True,
) -> list[str]:
    """Return stable diagnostics for malformed or stale inventory rows."""
    diagnostics: list[str] = []
    if not isinstance(inventory, dict):
        return ["generated artifact inventory must be a JSON object"]
    if inventory.get("schema") != SCHEMA:
        diagnostics.append(f"generated artifact inventory must use schema `{SCHEMA}`")
    artifacts = inventory.get("artifacts")
    if not isinstance(artifacts, list) or not artifacts:
        diagnostics.append("generated artifact inventory must contain non-empty `artifacts`")
        return diagnostics

    ids = [row.get("id") for row in artifacts if isinstance(row, dict)]
    if len(ids) != len(artifacts):
        diagnostics.append("every generated artifact row must be a JSON object with an `id`")
        return diagnostics
    if ids != sorted(ids):
        diagnostics.append("generated artifact rows must be sorted by `id`")
    duplicates = sorted({artifact_id for artifact_id in ids if ids.count(artifact_id) > 1})
    for artifact_id in duplicates:
        diagnostics.append(f"duplicate generated artifact id `{artifact_id}`")

    for row in artifacts:
        artifact_id = row.get("id", "<missing>")
        missing = sorted(REQUIRED_FIELDS.difference(row))
        if missing:
            diagnostics.append(
                f"generated artifact `{artifact_id}` is missing fields: {', '.join(missing)}"
            )
            continue
        unexpected = sorted(set(row).difference(REQUIRED_FIELDS))
        if unexpected:
            diagnostics.append(
                f"generated artifact `{artifact_id}` has undocumented fields: {', '.join(unexpected)}"
            )
        classification = row["classification"]
        if classification not in CLASSIFICATIONS:
            diagnostics.append(
                f"generated artifact `{artifact_id}` has invalid classification `{classification}`"
            )
        for field in ("id", "regenerate", "freshness_gate", "owner"):
            value = row[field]
            if not isinstance(value, str) or not value.strip():
                diagnostics.append(f"generated artifact `{artifact_id}` has blank `{field}`")
                continue
            normalized = value.lower()
            if any(term in normalized for term in FORBIDDEN_TERMS):
                diagnostics.append(
                    f"generated artifact `{artifact_id}` contains forbidden term in `{field}`"
                )
            if value.startswith(("/", "~")) or "\\" in value:
                diagnostics.append(
                    f"generated artifact `{artifact_id}` has host-dependent `{field}`"
                )
        gate = row["freshness_gate"]
        if gate not in targets:
            diagnostics.append(
                f"generated artifact `{artifact_id}` references missing Make target `{gate}`"
            )
        paths = row["paths"]
        if not isinstance(paths, list) or not paths:
            diagnostics.append(f"generated artifact `{artifact_id}` must declare non-empty `paths`")
            continue
        if paths != sorted(paths) or len(paths) != len(set(paths)):
            diagnostics.append(
                f"generated artifact `{artifact_id}` paths must be unique and sorted"
            )
        for pattern in paths:
            if not isinstance(pattern, str) or not pattern:
                diagnostics.append(f"generated artifact `{artifact_id}` has an invalid path")
                continue
            pure_path = PurePosixPath(pattern)
            if pure_path.is_absolute() or ".." in pure_path.parts or "\\" in pattern:
                diagnostics.append(
                    f"generated artifact `{artifact_id}` has unsafe path `{pattern}`"
                )
                continue
            if require_paths and not matched_artifact_paths(root, pattern):
                diagnostics.append(
                    f"generated artifact `{artifact_id}` path `{pattern}` matched no files"
                )
    return diagnostics


def inventoried_artifact_files(
    inventory: dict[str, Any], root: Path
) -> list[tuple[str, Path]]:
    """Expand inventoried artifact patterns into stable unique file rows."""
    rows: dict[Path, set[str]] = {}
    for artifact in inventory["artifacts"]:
        for pattern in artifact["paths"]:
            for path in matched_artifact_paths(root, pattern):
                if path.is_file():
                    rows.setdefault(path, set()).add(artifact["id"])
    return [
        (",".join(sorted(rows[path])), path)
        for path in sorted(rows, key=lambda item: item.as_posix())
    ]


def validate_artifact_contents(
    inventory: dict[str, Any], root: Path
) -> tuple[list[str], int]:
    """Reject host-local absolute paths embedded in generated artifacts."""
    diagnostics, snapshot = scan_artifact_snapshot(inventory, root)
    return diagnostics, len(snapshot)


def scan_artifact_snapshot(
    inventory: dict[str, Any], root: Path
) -> tuple[list[str], list[dict[str, Any]]]:
    """Read every artifact into a stable ownership and content snapshot."""
    diagnostics: list[str] = []
    snapshot: list[dict[str, Any]] = []
    rows = inventoried_artifact_files(inventory, root)
    for artifact_ids, path in rows:
        contents = path.read_bytes()
        relative = path.relative_to(root).as_posix()
        snapshot.append(
            {
                "artifact_ids": artifact_ids.split(","),
                "path": relative,
                "size_bytes": len(contents),
                "sha256": hashlib.sha256(contents).hexdigest(),
            }
        )
        for policy, pattern in HOST_PATH_PATTERNS.items():
            match = pattern.search(contents)
            if match is None:
                continue
            leaked = match.group().decode("utf-8", errors="replace")
            diagnostics.append(
                f"generated artifact `{artifact_ids}` file `{relative}` contains "
                f"host-local path ({policy}) `{leaked}`"
            )
    return diagnostics, snapshot


def snapshot_digest(snapshot: list[dict[str, Any]]) -> str:
    """Return one stable digest for a complete generated-artifact snapshot."""
    payload = json.dumps(snapshot, separators=(",", ":"), sort_keys=True).encode()
    return f"sha256:{hashlib.sha256(payload).hexdigest()}"


def report_payload(
    inventory_bytes: bytes,
    artifacts: list[dict[str, Any]],
    first_snapshot: list[dict[str, Any]],
    second_snapshot: list[dict[str, Any]],
    regeneration_run_count: int,
) -> dict[str, Any]:
    """Build the canonical report without timestamps or host-local paths."""
    classifications = {
        classification: sum(row["classification"] == classification for row in artifacts)
        for classification in sorted(CLASSIFICATIONS)
    }
    return {
        "schema": SCHEMA,
        "gate_id": "release-generated-artifacts-check",
        "input_digests": {
            INVENTORY_PATH.as_posix(): f"sha256:{hashlib.sha256(inventory_bytes).hexdigest()}"
        },
        "artifact_count": len(artifacts),
        "artifact_file_count": len(first_snapshot),
        "classifications": classifications,
        "content_policies": sorted(HOST_PATH_PATTERNS),
        "artifact_ids": [row["id"] for row in artifacts],
        "freshness_gates": sorted({row["freshness_gate"] for row in artifacts}),
        "hash_comparison": {
            "algorithm": "sha256",
            "file_count": len(first_snapshot),
            "combined_digest": snapshot_digest(first_snapshot),
        },
        "deterministic_run_comparison": {
            "run_count": 2,
            "first_snapshot_digest": snapshot_digest(first_snapshot),
            "second_snapshot_digest": snapshot_digest(second_snapshot),
            "identical": first_snapshot == second_snapshot,
        },
        "clean_regeneration_comparison": {
            "run_count": regeneration_run_count,
            "artifact_snapshot_preserved_after_each_run": regeneration_run_count == 2,
        },
        "drift_summary": {
            "changed_between_scans": 0,
            "freshness_gate_count": len({row["freshness_gate"] for row in artifacts}),
        },
        "decision": "pass",
    }


def canonical_json(payload: dict[str, Any]) -> bytes:
    """Render reports with stable key ordering and a terminating newline."""
    return (json.dumps(payload, indent=2, sort_keys=True) + "\n").encode()


def run(root: Path, regeneration_run_count: int) -> Path:
    """Validate inventory and write its deterministic quality report."""
    inventory_file = root / INVENTORY_PATH
    inventory_bytes = inventory_file.read_bytes()
    inventory = json.loads(inventory_bytes)
    targets = repository_make_targets(root)
    makefile_text = (root / "Makefile").read_text()
    diagnostics = validate_inventory(inventory, root, targets)
    diagnostics.extend(validate_aggregate_recipe(inventory, makefile_text))
    first_diagnostics, first_snapshot = scan_artifact_snapshot(inventory, root)
    diagnostics.extend(first_diagnostics)
    if diagnostics:
        raise ValueError("release generated artifact inventory failed:\n- " + "\n- ".join(diagnostics))
    second_diagnostics, second_snapshot = scan_artifact_snapshot(inventory, root)
    if second_diagnostics:
        raise ValueError(
            "release generated artifact second scan failed:\n- "
            + "\n- ".join(second_diagnostics)
        )
    if first_snapshot != second_snapshot:
        raise ValueError(
            "release generated artifacts changed between deterministic scans: "
            f"{snapshot_digest(first_snapshot)} != {snapshot_digest(second_snapshot)}"
        )
    report = canonical_json(
        report_payload(
            inventory_bytes,
            inventory["artifacts"],
            first_snapshot,
            second_snapshot,
            regeneration_run_count,
        )
    )
    report_path = root / REPORT_PATH
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_bytes(report)
    return report_path


def current_snapshot(root: Path) -> list[dict[str, Any]]:
    """Return a validated snapshot for all inventoried generated artifacts."""

    inventory = json.loads((root / INVENTORY_PATH).read_bytes())
    diagnostics = validate_inventory(inventory, root, repository_make_targets(root))
    content_diagnostics, snapshot = scan_artifact_snapshot(inventory, root)
    diagnostics.extend(content_diagnostics)
    if diagnostics:
        raise ValueError("release generated artifact snapshot failed:\n- " + "\n- ".join(diagnostics))
    return snapshot


def record_snapshot(root: Path, path: Path) -> None:
    """Persist the generated-artifact state before isolated regeneration."""

    destination = root / path
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(canonical_json({"snapshot": current_snapshot(root)}))


def compare_snapshot(root: Path, path: Path) -> None:
    """Reject any generated-artifact change after one regeneration pass."""

    expected_path = root / path
    expected = json.loads(expected_path.read_bytes()).get("snapshot")
    actual = current_snapshot(root)
    if expected != actual:
        raise ValueError(
            "release generated artifacts changed during regeneration: "
            f"{snapshot_digest(expected or [])} != {snapshot_digest(actual)}"
        )


def self_test() -> None:
    """Exercise adversarial inventory validation without touching repository files."""
    base = {
        "schema": SCHEMA,
        "artifacts": [
            {
                "id": "fixture",
                "classification": "committed",
                "paths": ["fixture.txt"],
                "regenerate": "make fixture-check",
                "freshness_gate": "fixture-check",
                "owner": "release-tooling",
            }
        ],
    }
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        (root / "fixture.txt").write_text("fixture\n")
        targets = {"fixture-check"}
        assert not validate_inventory(base, root, targets)
        duplicate = copy.deepcopy(base)
        duplicate["artifacts"].append(copy.deepcopy(duplicate["artifacts"][0]))
        assert any("duplicate generated artifact id" in item for item in validate_inventory(duplicate, root, targets))
        invalid_class = copy.deepcopy(base)
        invalid_class["artifacts"][0]["classification"] = "workspace-maybe"
        assert any("invalid classification" in item for item in validate_inventory(invalid_class, root, targets))
        unsafe_path = copy.deepcopy(base)
        unsafe_path["artifacts"][0]["paths"] = ["../fixture.txt"]
        assert any("unsafe path" in item for item in validate_inventory(unsafe_path, root, targets))
        missing_gate = copy.deepcopy(base)
        assert any("missing Make target" in item for item in validate_inventory(missing_gate, root, set()))
        missing_path = copy.deepcopy(base)
        missing_path["artifacts"][0]["paths"] = ["missing.txt"]
        assert any("matched no files" in item for item in validate_inventory(missing_path, root, targets))
        (root / "fixture.txt").write_text("generated by /home/alice/project/terlc\n")
        diagnostics, file_count = validate_artifact_contents(base, root)
        assert file_count == 1
        assert any("host-local path (unix-home)" in item for item in diagnostics)
        (root / "fixture.txt").write_text(r"generated by C:\Users\alice\terlc.exe")
        diagnostics, _ = validate_artifact_contents(base, root)
        assert any("host-local path (windows-user)" in item for item in diagnostics)
        (root / "fixture.txt").write_text("portable generated artifact\n")
        diagnostics, _ = validate_artifact_contents(base, root)
        assert not diagnostics
        aggregate = """release-generated-artifacts-freshness-pass:
\t$(MAKE) --no-print-directory fixture-check

release-generated-artifacts-check:
\t$(PYTHON) tools/check_release_generated_artifacts.py --record-snapshot target/quality/release-generated-artifacts-before.json
\t$(MAKE) --no-print-directory release-generated-artifacts-freshness-pass
\t$(PYTHON) tools/check_release_generated_artifacts.py --compare-snapshot target/quality/release-generated-artifacts-before.json
\t$(MAKE) --no-print-directory release-generated-artifacts-freshness-pass
\t$(PYTHON) tools/check_release_generated_artifacts.py --compare-snapshot target/quality/release-generated-artifacts-before.json
\t$(PYTHON) tools/check_release_generated_artifacts.py --self-test
\t$(PYTHON) tools/check_release_generated_artifacts.py --regeneration-run-count 2

fixture-check:
\t@true
"""
        assert not validate_aggregate_recipe(base, aggregate)
        assert validate_aggregate_recipe(base, aggregate.replace("fixture-check", "other-check"))
        _, first_snapshot = scan_artifact_snapshot(base, root)
        _, second_snapshot = scan_artifact_snapshot(base, root)
        assert first_snapshot == second_snapshot
        first_digest = snapshot_digest(first_snapshot)
        (root / "fixture.txt").write_text("changed portable generated artifact\n")
        _, changed_snapshot = scan_artifact_snapshot(base, root)
        assert first_snapshot != changed_snapshot
        assert first_digest != snapshot_digest(changed_snapshot)
        report = report_payload(
            b"inventory", base["artifacts"], first_snapshot, second_snapshot, 2
        )
        assert report["deterministic_run_comparison"]["identical"] is True
        assert report["drift_summary"]["changed_between_scans"] == 0
        assert report["clean_regeneration_comparison"] == {
            "run_count": 2,
            "artifact_snapshot_preserved_after_each_run": True,
        }
        inventory_path = root / INVENTORY_PATH
        inventory_path.parent.mkdir(parents=True)
        inventory_path.write_bytes(canonical_json(base))
        (root / "Makefile").write_text(aggregate)
        snapshot_path = Path("snapshot.json")
        record_snapshot(root, snapshot_path)
        compare_snapshot(root, snapshot_path)
        (root / "fixture.txt").write_text("snapshot drift\n")
        try:
            compare_snapshot(root, snapshot_path)
        except ValueError as error:
            assert "changed during regeneration" in str(error)
        else:
            raise AssertionError("snapshot drift must fail")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--record-snapshot", type=Path)
    parser.add_argument("--compare-snapshot", type=Path)
    parser.add_argument("--regeneration-run-count", type=int, default=0)
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("release generated artifact inventory self-tests passed")
        return 0
    try:
        if args.record_snapshot is not None:
            record_snapshot(Path("."), args.record_snapshot)
            print(f"release generated artifact snapshot recorded: {args.record_snapshot}")
            return 0
        if args.compare_snapshot is not None:
            compare_snapshot(Path("."), args.compare_snapshot)
            print(f"release generated artifact snapshot preserved: {args.compare_snapshot}")
            return 0
        if args.regeneration_run_count != 2:
            raise ValueError("release generated artifact report requires exactly two regeneration runs")
        report_path = run(Path("."), args.regeneration_run_count)
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(error)
        return 1
    print(f"release generated artifact inventory passed; report written to {report_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
