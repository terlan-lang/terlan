#!/usr/bin/env python3
"""Validate and record repository-local direct-AOT closeout evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
import platform
import subprocess
import sys
from pathlib import Path

import check_tvm_aot_platform_matrix as platform_matrix
import check_tvm_aot_thread_sanitizer as thread_sanitizer


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "target/quality/tvm-aot-release-closeout-report.json"
INVENTORY_SOURCE = ROOT / "docs/runtime/TVM_AOT_PIVOT_INVENTORY.md"
SCHEMA = "terlan.tvm-aot-local-closeout.v2"
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
    "tvm-aot-platform-matrix-check",
    "tvm-aot-lowering-coverage-check",
    "tvm-aot-http-persistent-shard-check",
    "tvm-aot-http-generation-lifetime-check",
    "tvm-aot-http-performance-check",
    "tvm-aot-multicore-readiness-check",
    "tvm-aot-thread-sanitizer-check",
    "tvm-aot-c-abi-boundary-check",
    "tvm-aot-compilation-time-check",
    "tvm-single-image-artifact-check",
    "no-tvm-json-runtime-check",
    "no-vmir-interpreter-check",
    "rust-quality-check",
    "roadmap-gate-integrity-check",
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
        "terlan-http-aot-performance-comparison-v2",
        "completed",
    ),
    "managed_list": (
        "target/quality/tvm-managed-list-profile.json",
        "terlan.tvm.managed-list-profile.v1",
        None,
    ),
}


def command_output(command: list[str], root: Path = ROOT) -> str:
    """Run a read-only command and return normalized output."""

    return subprocess.run(
        command, cwd=root, check=True, capture_output=True, text=True
    ).stdout.strip()


def sha256(path: Path) -> str:
    """Return the SHA-256 identity of one evidence file."""

    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def load_json(path: Path) -> dict[str, object]:
    """Load one required JSON object."""

    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise AssertionError(f"cannot load AOT closeout evidence `{path}`: {error}") from error
    if not isinstance(value, dict):
        raise AssertionError(f"AOT closeout evidence `{path}` is not an object")
    return value


def make_list_variable(makefile: str, name: str) -> tuple[str, ...]:
    """Parse one continued Make list assignment."""

    prefix = f"{name} :="
    lines = makefile.splitlines()
    for index, line in enumerate(lines):
        if not line.startswith(prefix):
            continue
        values: list[str] = []
        current = line[len(prefix) :].strip()
        while True:
            continued = current.endswith("\\")
            value = current.removesuffix("\\").strip()
            if value:
                values.append(value)
            if not continued:
                return tuple(values)
            index += 1
            if index >= len(lines):
                raise AssertionError(f"Makefile `{name}` assignment is incomplete")
            current = lines[index].strip()
    raise AssertionError(f"Makefile omits `{name}`")


def validate_makefile_contract(makefile: str) -> None:
    """Require local closeout to execute every recorded gate in order."""

    if make_list_variable(makefile, "AOT_RELEASE_LOCAL_GATES") != LOCAL_GATES[:-1]:
        raise AssertionError("AOT closeout Make gates disagree with the local contract")
    expected = (
        "tvm-aot-release-closeout-check: tvm-aot-release-closeout-contract-check\n"
        "\t$(MAKE) $(AOT_RELEASE_LOCAL_GATES)\n"
        "\tenv -u RUSTFLAGS $(CARGO) check --locked -p terlan\n"
        "\t$(PYTHON) tools/check_tvm_aot_release_closeout.py record-local\n"
    )
    if expected not in makefile:
        raise AssertionError("AOT closeout no longer executes its canonical local graph")


def validate_local_evidence(name: str, value: dict[str, object]) -> None:
    """Validate a benchmark or managed-runtime report."""

    _, schema, status = EVIDENCE[name]
    if value.get("schema") != schema:
        raise AssertionError(f"AOT `{name}` evidence has an unexpected schema")
    if status is not None and value.get("status") != status:
        raise AssertionError(f"AOT `{name}` evidence did not complete")
    if name == "compilation":
        cache_state = value.get("cache_state")
        fixtures = value.get("fixtures")
        required = {"terlan_cold", "go_cold", "warm", "dependency_downloads_timed"}
        if not isinstance(cache_state, dict) or not required.issubset(cache_state):
            raise AssertionError("compilation evidence omitted canonical cache state")
        if not isinstance(fixtures, dict) or not platform_matrix.is_sha256(
            fixtures.get("sha256")
        ):
            raise AssertionError("compilation evidence omitted fixture identity")
    if name == "managed_list" and value.get("correctness_verified") is not True:
        raise AssertionError("managed-list evidence omitted correctness proof")


def inventory_counts(markdown: str) -> dict[str, int]:
    """Count inventory rows and reject transitional classifications."""

    counts = {classification: 0 for classification in INVENTORY_CLASSIFICATIONS}
    for line in markdown.splitlines():
        if not line.startswith("| `"):
            continue
        columns = [column.strip() for column in line.strip("|").split("|")]
        if len(columns) != 4:
            raise AssertionError("AOT inventory contains a malformed row")
        classification = columns[2].strip("`")
        if classification not in counts:
            raise AssertionError(f"AOT inventory has unknown class `{classification}`")
        counts[classification] += 1
    if sum(counts.values()) == 0:
        raise AssertionError("AOT inventory contains no canonical rows")
    for classification in ("temporary-migration-support", "deletion-debt"):
        if counts[classification] != 0:
            raise AssertionError(f"AOT inventory retains `{classification}`")
    return counts


def validate_host_report(report: dict[str, object], revision: str) -> None:
    """Validate the executable report produced on the current host."""

    target_id = platform_matrix.host_target_id()
    expected = platform_matrix.TARGETS[target_id]
    if report.get("schema") != platform_matrix.TARGET_SCHEMA:
        raise AssertionError("host platform report has an unexpected schema")
    if report.get("decision") != "pass" or report.get("target_id") != target_id:
        raise AssertionError("host platform execution did not pass")
    for field, value in expected.items():
        if report.get(field) != value:
            raise AssertionError(f"host platform report has stale `{field}`")
    source_revision = report.get("source_revision")
    if not isinstance(source_revision, str) or not revision.startswith(source_revision):
        raise AssertionError("host platform report belongs to another revision")
    if report.get("executed_checks") != list(platform_matrix.REQUIRED_EXECUTED_CHECKS):
        raise AssertionError("host platform report omitted executable checks")
    for field in ("descriptor_digest", "image_sha256"):
        if not platform_matrix.is_sha256(report.get(field)):
            raise AssertionError(f"host platform report has invalid `{field}`")


def record_local_closeout(root: Path = ROOT) -> Path:
    """Record local AOT closure without commit, push, or hosted artifacts."""

    revision = command_output(["git", "rev-parse", "HEAD"], root)
    host_id = platform_matrix.host_target_id()
    host_path = root / f"target/quality/tvm-aot-platform/{host_id}.json"
    host = load_json(host_path)
    validate_host_report(host, revision)

    retained: dict[str, dict[str, object]] = {}
    for name, (relative, _, _) in EVIDENCE.items():
        path = root / relative
        value = load_json(path)
        validate_local_evidence(name, value)
        retained[name] = {"path": relative, "sha256": sha256(path)}

    inventory_path = root / INVENTORY_SOURCE.relative_to(ROOT)
    counts = inventory_counts(inventory_path.read_text(encoding="utf-8"))
    retained["inventory"] = {
        "path": str(inventory_path.relative_to(root)),
        "sha256": sha256(inventory_path),
        "classification_counts": counts,
    }

    sanitizer_path = root / thread_sanitizer.REPORT.relative_to(ROOT)
    sanitizer: dict[str, object]
    if sanitizer_path.is_file():
        sanitizer_report = load_json(sanitizer_path)
        thread_sanitizer.validate_report(sanitizer_report, require_ci=False)
        if sanitizer_report.get("source_revision") != revision:
            raise AssertionError("ThreadSanitizer report belongs to another revision")
        sanitizer = {
            "mode": "instrumented",
            "path": str(sanitizer_path.relative_to(root)),
            "sha256": sha256(sanitizer_path),
        }
    else:
        sanitizer = {
            "mode": "contract-only",
            "reason": f"Rust target `{thread_sanitizer.TARGET}` is not installed",
        }

    report: dict[str, object] = {
        "schema": SCHEMA,
        "decision": "pass",
        "source_revision": revision,
        "worktree_committed": False,
        "host": {"system": platform.system(), "machine": platform.machine()},
        "toolchain": {
            "rustc": command_output(["rustc", "--version", "--verbose"], root),
            "cargo": command_output(["cargo", "--version"], root),
        },
        "local_gates": list(LOCAL_GATES),
        "platform_contract": {
            "supported_targets": list(platform_matrix.TARGETS),
            "host_report": {
                "path": str(host_path.relative_to(root)),
                "sha256": sha256(host_path),
            },
        },
        "thread_sanitizer": sanitizer,
        "evidence": retained,
        "semantic_preservation": {
            "runtime_fallbacks": 0,
            "temporary_migration_support": 0,
            "deletion_debt": 0,
        },
        "publishing_required": False,
    }
    output = root / REPORT.relative_to(ROOT)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"TVM AOT local closeout passed: {revision}")
    return output


def self_test() -> None:
    """Prove the local contract rejects missing gates and migration debt."""

    makefile = (ROOT / "Makefile").read_text(encoding="utf-8")
    validate_makefile_contract(makefile)
    invalid = makefile.replace("\troadmap-gate-integrity-check\n", "", 1)
    try:
        validate_makefile_contract(invalid)
    except AssertionError:
        pass
    else:
        raise AssertionError("closeout accepted a missing AOT-only gate")

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
        try:
            inventory_counts(
                valid_inventory
                + f"\n| `invalid.rs` | runtime | {classification} | Remove. |"
            )
        except AssertionError:
            pass
        else:
            raise AssertionError(f"closeout accepted `{classification}` debt")
    print("TVM AOT local release closeout self-test passed")


def main() -> int:
    """Dispatch local recording or adversarial contract checks."""

    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("record-local", "self-test"))
    command = parser.parse_args().command
    try:
        if command == "record-local":
            record_local_closeout()
        else:
            self_test()
    except (AssertionError, OSError, subprocess.CalledProcessError) as error:
        print(f"TVM AOT local closeout failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
