#!/usr/bin/env python3
"""Seal complete direct-AOT release evidence into one provenance record."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import subprocess
import sys
import tempfile
from pathlib import Path

import check_tvm_aot_platform_matrix as platform_matrix
import check_tvm_aot_thread_sanitizer as thread_sanitizer


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "target/quality/tvm-aot-release-closeout-report.json"
CLEAN_REPORT = ROOT / "target/quality/tvm-aot-release-clean-checkout.json"
INVENTORY_SOURCE = ROOT / "docs/runtime/TVM_AOT_PIVOT_INVENTORY.md"
SCHEMA = "terlan.tvm-aot-release-closeout.v1"
CLEAN_SCHEMA = "terlan.tvm-aot-release-clean-checkout.v1"
INVENTORY_CLASSIFICATIONS = (
    "compiler-internal-ir",
    "deletion-debt",
    "reusable-runtime-semantics",
    "temporary-migration-support",
)
LOCAL_GATES = (
    "runtime-aot-only-check",
    "tvm-direct-aot-backend-check",
    "tvm-managed-memory-check",
    "tvm-managed-list-profile-benchmark-check",
    "terlan-vm-artifact-format-check",
    "tvm-native-image-format-check",
    "tvm-native-image-loader-check",
    "tvm-aot-consumer-check",
    "tvm-aot-package-install-consumer-check",
    "tvm-aot-runtime-transition-check",
    "tvm-aot-shard-ownership-check",
    "tvm-aot-supervisor-lifecycle-check",
    "tvm-aot-stale-epoch-check",
    "tvm-aot-crash-injection-check",
    "tvm-aot-capability-worker-check",
    "tvm-aot-image-lifetime-check",
    "tvm-aot-lowering-coverage-check",
    "tvm-aot-http-generation-lifetime-check",
    "tvm-aot-http-performance-check",
    "tvm-aot-multicore-readiness-check",
    "tvm-aot-c-abi-boundary-check",
    "tvm-aot-compilation-time-check",
    "tvm-single-image-artifact-check",
    "no-tvm-json-runtime-check",
    "no-vmir-interpreter-check",
    "rust-quality-check",
    "roadmap-gate-integrity-check",
    "check",
    "cargo-check-locked-terlan",
)
EVIDENCE = {
    "compilation": (
        "target/quality/aot-compilation-benchmark.json",
        "terlan-aot-compilation-benchmark-v1",
        "completed",
    ),
    "http_performance": (
        "target/quality/http-aot-performance-comparison.json",
        "terlan-http-aot-performance-comparison-v1",
        "completed",
    ),
    "managed_list": (
        "target/quality/tvm-managed-list-profile.json",
        "terlan.tvm.managed-list-profile.v1",
        None,
    ),
}


def command_output(command: list[str], root: Path = ROOT) -> str:
    """Run one read-only command and return normalized standard output."""

    return subprocess.run(
        command,
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def sha256(path: Path) -> str:
    """Return the lowercase SHA-256 identity of one evidence file."""

    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load_json(path: Path) -> dict[str, object]:
    """Load one required JSON object with a path-specific diagnostic."""

    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise AssertionError(f"cannot load closeout evidence `{path}`: {error}") from error
    if not isinstance(value, dict):
        raise AssertionError(f"closeout evidence `{path}` is not a JSON object")
    return value


def validate_external_evidence(
    matrix: dict[str, object],
    sanitizer: dict[str, object],
    revision: str,
    require_ci: bool,
) -> None:
    """Require platform and race evidence for the exact release revision."""

    if matrix.get("schema") != platform_matrix.MATRIX_SCHEMA or matrix.get("decision") != "pass":
        raise AssertionError("release closeout requires one passing six-target platform matrix")
    if matrix.get("target_count") != 6 or matrix.get("static_or_skipped_rows") != 0:
        raise AssertionError("release platform evidence is incomplete or contains skipped rows")
    if matrix.get("commit_sha") != revision:
        raise AssertionError("release platform matrix belongs to another commit")
    targets = matrix.get("targets")
    if not isinstance(targets, list) or any(not isinstance(row, dict) for row in targets):
        raise AssertionError("release platform matrix omitted target artifact evidence")
    reports = {str(row.get("target_id")): row for row in targets}
    if len(reports) != len(targets) or set(reports) != set(platform_matrix.TARGETS):
        raise AssertionError("release platform matrix has missing or duplicate target evidence")
    if matrix != platform_matrix.build_matrix(reports):
        raise AssertionError("release platform matrix is not its canonical aggregate")
    thread_sanitizer.validate_report(sanitizer, require_ci=require_ci)
    if sanitizer.get("source_revision") != revision:
        raise AssertionError("ThreadSanitizer evidence belongs to another commit")
    if require_ci:
        for field in ("repository", "run_id", "run_attempt"):
            if matrix.get(field) != sanitizer.get(field):
                raise AssertionError(
                    f"platform and ThreadSanitizer evidence disagree on `{field}`"
                )


def validate_local_evidence(name: str, value: dict[str, object]) -> None:
    """Validate one benchmark, cache, inventory, or semantic evidence object."""

    _, schema, status = EVIDENCE[name]
    if value.get("schema") != schema:
        raise AssertionError(f"closeout `{name}` evidence has an unexpected schema")
    if status is not None and value.get("status") != status:
        raise AssertionError(f"closeout `{name}` evidence did not complete")
    if name == "compilation":
        cache_state = value.get("cache_state")
        fixtures = value.get("fixtures")
        if not isinstance(cache_state, dict) or not {
            "terlan_cold",
            "go_cold",
            "warm",
            "dependency_downloads_timed",
        }.issubset(cache_state):
            raise AssertionError("compilation evidence omitted canonical cache state")
        if not isinstance(fixtures, dict) or not platform_matrix.is_sha256(
            fixtures.get("sha256")
        ):
            raise AssertionError("compilation evidence omitted fixture identity")
    if name == "managed_list" and value.get("correctness_verified") is not True:
        raise AssertionError("managed-list profile omitted correctness evidence")


def inventory_counts(markdown: str) -> dict[str, int]:
    """Count canonical inventory rows and reject malformed classifications."""

    counts = {classification: 0 for classification in INVENTORY_CLASSIFICATIONS}
    rows = 0
    for line in markdown.splitlines():
        if not line.startswith("| `"):
            continue
        columns = [column.strip() for column in line.strip("|").split("|")]
        if len(columns) != 4:
            raise AssertionError("AOT inventory contains a malformed table row")
        classification = columns[2].strip("`")
        if classification not in counts:
            raise AssertionError(
                f"AOT inventory contains unknown classification `{classification}`"
            )
        counts[classification] += 1
        rows += 1
    if rows == 0:
        raise AssertionError("AOT inventory contains no canonical rows")
    for classification in ("temporary-migration-support", "deletion-debt"):
        if counts[classification] != 0:
            raise AssertionError(f"AOT inventory retains `{classification}` rows")
    return counts


def require_clean_checkout(root: Path) -> None:
    """Reject release evidence produced from tracked or untracked source changes."""

    if command_output(["git", "status", "--porcelain=v1"], root=root):
        raise AssertionError("AOT release closeout requires a clean Git checkout")


def record_clean_checkout(root: Path = ROOT) -> Path:
    """Prove and retain the clean source revision before running closeout gates."""

    require_clean_checkout(root)
    revision = command_output(["git", "rev-parse", "HEAD"], root=root)
    report = {
        "schema": CLEAN_SCHEMA,
        "decision": "pass",
        "source_revision": revision,
    }
    output = root / CLEAN_REPORT.relative_to(ROOT)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"TVM AOT clean checkout passed: {revision}")
    return output


def record_closeout(root: Path = ROOT, require_ci: bool = True) -> Path:
    """Validate all retained evidence and write one canonical closeout record."""

    require_clean_checkout(root)
    revision = command_output(["git", "rev-parse", "HEAD"], root=root)
    clean_path = root / CLEAN_REPORT.relative_to(ROOT)
    clean = load_json(clean_path)
    if clean != {
        "schema": CLEAN_SCHEMA,
        "decision": "pass",
        "source_revision": revision,
    }:
        raise AssertionError("AOT closeout clean-checkout evidence is stale or incomplete")
    matrix_path = root / "target/quality/tvm-aot-platform-matrix-report.json"
    sanitizer_path = root / "target/quality/tvm-aot-thread-sanitizer-report.json"
    matrix = load_json(matrix_path)
    sanitizer = load_json(sanitizer_path)
    validate_external_evidence(matrix, sanitizer, revision, require_ci=require_ci)

    retained: dict[str, dict[str, object]] = {}
    local_values: dict[str, dict[str, object]] = {}
    for name, (relative, _, _) in EVIDENCE.items():
        path = root / relative
        value = load_json(path)
        validate_local_evidence(name, value)
        local_values[name] = value
        retained[name] = {"path": relative, "sha256": sha256(path)}

    inventory_path = root / INVENTORY_SOURCE.relative_to(ROOT)
    counts = inventory_counts(inventory_path.read_text(encoding="utf-8"))
    retained["inventory"] = {
        "path": str(inventory_path.relative_to(root)),
        "sha256": sha256(inventory_path),
        "classification_counts": counts,
        "row_count": sum(counts.values()),
    }

    report: dict[str, object] = {
        "schema": SCHEMA,
        "decision": "pass",
        "source_revision": revision,
        "host": {
            "system": platform.system(),
            "machine": platform.machine(),
        },
        "toolchain": {
            "rustc": command_output(["rustc", "--version", "--verbose"], root=root),
            "cargo": command_output(["cargo", "--version"], root=root),
        },
        "local_gates": list(LOCAL_GATES),
        "clean_checkout": {
            "path": str(clean_path.relative_to(root)),
            "sha256": sha256(clean_path),
        },
        "platform_matrix": {
            "path": str(matrix_path.relative_to(root)),
            "sha256": sha256(matrix_path),
            "run_id": matrix.get("run_id"),
            "run_attempt": matrix.get("run_attempt"),
        },
        "thread_sanitizer": {
            "path": str(sanitizer_path.relative_to(root)),
            "sha256": sha256(sanitizer_path),
        },
        "retained_evidence": retained,
        "cache_evidence": {
            "source": retained["compilation"],
            "state": local_values["compilation"]["cache_state"],
        },
        "artifact_evidence": {
            "source": {
                "path": str(matrix_path.relative_to(root)),
                "sha256": sha256(matrix_path),
            },
            "targets": [
                {
                    "target_id": row["target_id"],
                    "descriptor_digest": row["descriptor_digest"],
                    "image_sha256": row["image_sha256"],
                }
                for row in matrix["targets"]
            ],
        },
        "semantic_preservation": {
            "runtime_fallbacks": 0,
            "temporary_migration_support": 0,
            "deletion_debt": 0,
        },
    }
    output = root / REPORT.relative_to(ROOT)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"TVM AOT release closeout passed: {revision}")
    return output


def self_test() -> None:
    """Prove mixed revisions, skipped targets, and migration debt fail closed."""

    revision = "a" * 40
    target_reports: dict[str, dict[str, object]] = {}
    for target_id, expected in platform_matrix.TARGETS.items():
        target_reports[target_id] = {
            "schema": platform_matrix.TARGET_SCHEMA,
            "decision": "pass",
            "target_id": target_id,
            **expected,
            "version": "self-test",
            "source_revision": revision,
            "execution_environment": "github-actions",
            "repository": platform_matrix.OFFICIAL_REPOSITORY,
            "workflow_ref": "terlan-lang/terlan/.github/workflows/release.yml@refs/tags/v0.0.7",
            "run_id": 9,
            "run_attempt": 1,
            "commit_sha": revision,
            "descriptor_digest": "ab" * 32,
            "image_sha256": "cd" * 32,
            "continuation_ids": [1],
            "native_debug_record_count": 2,
            "executed_checks": list(platform_matrix.REQUIRED_EXECUTED_CHECKS),
        }
    matrix = platform_matrix.build_matrix(target_reports)
    sanitizer: dict[str, object] = {
        "schema": thread_sanitizer.SCHEMA,
        "decision": "pass",
        "host": "linux-x86_64",
        "instrumented_target": thread_sanitizer.TARGET,
        "test_filter": thread_sanitizer.TEST_FILTER,
        "source_revision": revision,
        "rustc": "rustc 1.96.0\nbinary: rustc",
        "execution_environment": "github-actions",
        "repository": platform_matrix.OFFICIAL_REPOSITORY,
        "workflow_ref": "terlan-lang/terlan/.github/workflows/release.yml@refs/tags/v0.0.7",
        "run_id": 9,
        "run_attempt": 1,
        "commit_sha": revision,
    }
    validate_external_evidence(matrix, sanitizer, revision, require_ci=True)
    missing_targets = dict(matrix)
    missing_targets["targets"] = []
    try:
        validate_external_evidence(
            missing_targets, sanitizer, revision, require_ci=True
        )
    except AssertionError:
        pass
    else:
        raise AssertionError("closeout accepted missing target artifact evidence")
    for owner, field, value in (
        (matrix, "target_count", 5),
        (matrix, "static_or_skipped_rows", 1),
        (matrix, "commit_sha", "b" * 40),
        (sanitizer, "run_id", 10),
    ):
        invalid_matrix = dict(matrix)
        invalid_sanitizer = dict(sanitizer)
        target = invalid_matrix if owner is matrix else invalid_sanitizer
        target[field] = value
        try:
            validate_external_evidence(
                invalid_matrix, invalid_sanitizer, revision, require_ci=True
            )
        except AssertionError:
            pass
        else:
            raise AssertionError(f"closeout accepted invalid external `{field}`")

    valid_inventory = "\n".join(
        (
            "| Path | Surface | Classification | Disposition |",
            "| --- | --- | --- | --- |",
            "| `runtime.rs` | runtime | reusable-runtime-semantics | Keep. |",
            "| `compiler.rs` | compiler | compiler-internal-ir | Keep. |",
        )
    )
    assert inventory_counts(valid_inventory)["reusable-runtime-semantics"] == 1
    for classification in ("temporary-migration-support", "deletion-debt"):
        invalid = (
            valid_inventory
            + f"\n| `invalid.rs` | runtime | {classification} | Remove. |"
        )
        try:
            inventory_counts(invalid)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"closeout accepted `{classification}` debt")
    for invalid in (
        "",
        "| `runtime.rs` | runtime | unknown | Keep. |",
        "| `runtime.rs` | runtime | reusable-runtime-semantics |",
    ):
        try:
            inventory_counts(invalid)
        except AssertionError:
            pass
        else:
            raise AssertionError("closeout accepted a malformed AOT inventory")

    valid_compilation = {
        "schema": EVIDENCE["compilation"][1],
        "status": EVIDENCE["compilation"][2],
        "cache_state": {
            "terlan_cold": "fresh",
            "go_cold": "fresh",
            "warm": "populated",
            "dependency_downloads_timed": False,
        },
        "fixtures": {"sha256": "ab" * 32},
    }
    validate_local_evidence("compilation", valid_compilation)
    for field in ("cache_state", "fixtures"):
        invalid = dict(valid_compilation)
        invalid[field] = {}
        try:
            validate_local_evidence("compilation", invalid)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"closeout accepted compilation evidence without `{field}`")

    with tempfile.TemporaryDirectory(prefix="terlan-aot-closeout-self-test.") as tmp:
        root = Path(tmp)
        subprocess.run(["git", "init", "--quiet"], cwd=root, check=True)
        (root / "dirty.txt").write_text("dirty\n", encoding="utf-8")
        try:
            require_clean_checkout(root)
        except AssertionError:
            pass
        else:
            raise AssertionError("closeout accepted an untracked dirty checkout")
    print("TVM AOT release closeout self-test passed")


def main() -> int:
    """Dispatch release closeout recording or adversarial contract checks."""

    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("precheck", "record", "self-test"))
    command = parser.parse_args().command
    try:
        if command == "precheck":
            record_clean_checkout()
        elif command == "record":
            record_closeout(
                require_ci=os.environ.get("GITHUB_ACTIONS", "").lower() == "true"
            )
        else:
            self_test()
    except (AssertionError, OSError, subprocess.CalledProcessError) as error:
        print(f"TVM AOT release closeout failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
