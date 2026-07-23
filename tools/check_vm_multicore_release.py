#!/usr/bin/env python3
"""Validate the distributed multicore release-check composition."""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from collections.abc import Callable
from pathlib import Path

import check_tvm_aot_platform_matrix as platform_matrix
import check_vm_multicore_mc9_evidence as mc9


ROOT = Path(__file__).resolve().parents[1]
OUTPUT_REPORT = ROOT / "target/quality/vm-multicore-release-closeout.json"
MC9_REPORT = ROOT / "target/quality/vm-multicore-mc9-evidence.json"
PLATFORM_REPORT = ROOT / "target/quality/tvm-aot-platform-matrix-report.json"
INVARIANT_INVENTORY = ROOT / "docs/runtime/TVM_MULTICORE_INVARIANT_INVENTORY.json"
CONCURRENCY_CONTRACT = ROOT / "docs/runtime/TVM_MULTICORE_CONCURRENCY_CONTRACT.md"
SCHEMA = "terlan.vm-multicore-release-closeout.v1"
INVARIANT_SCHEMA = "terlan.tvm-multicore-invariant-inventory.v1"
INVARIANT_REVISION_DOMAIN = b"terlan.vm-multicore-invariants.v1\0"
LOCAL_GATES = (
    "vm-multicore-invariant-inventory-check",
    "vm-actor-mutator-ownership-check",
    "vm-multicore-mailbox-publication-check",
    "vm-multicore-fixed-placement-check",
    "tvm-aot-multicore-migration-check",
    "vm-multicore-work-stealing-check",
    "vm-multicore-runtime-cleanup-check",
    "vm-multicore-runtime-integration-check",
    "vm-epmd-discovery-check",
    "vm-multicore-replay-observability-check",
    "vm-multicore-memory-model-check",
    "vm-scheduler-fairness-check",
    "tvm-aot-runtime-transition-check",
    "tvm-managed-memory-check",
    "rust-quality-check",
    "roadmap-gate-integrity-check",
    "check",
)
RELEASE_TARGET = (
    "vm-multicore-release-check: vm-multicore-release-contract-check\n"
    "\t$(MAKE) vm-multicore-mc9-evidence-check\n"
    "\t$(MAKE) $(VM_MULTICORE_RELEASE_LOCAL_GATES)\n"
    "\t$(PYTHON) tools/check_vm_multicore_release.py record\n"
    "\ttest -s target/quality/vm-multicore-release-closeout.json\n"
    "\t@rg -q '\"schema\": \"terlan.vm-multicore-release-closeout.v1\"'"
    " target/quality/vm-multicore-release-closeout.json\n"
    "\t@rg -q '\"decision\": \"pass\"' target/quality/vm-multicore-release-closeout.json\n"
)
WORKFLOW_PRODUCERS = (
    "run: make vm-multicore-thread-sanitizer-check",
    "run: make vm-multicore-performance-check",
)


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


def validate_makefile(makefile: str) -> None:
    """Require the canonical target to consume evidence before local gates."""

    gates = make_list_variable(makefile, "VM_MULTICORE_RELEASE_LOCAL_GATES")
    if gates != LOCAL_GATES:
        raise AssertionError("multicore release gates disagree with the contract")
    if RELEASE_TARGET not in makefile:
        raise AssertionError("multicore release target is not canonical")
    if "vm-multicore-performance-check" in gates:
        raise AssertionError("release validation would overwrite controlled evidence")
    if "vm-multicore-thread-sanitizer-check" in gates:
        raise AssertionError("release validation would overwrite sanitizer evidence")


def validate_workflow(workflow: str) -> None:
    """Require distributed producers and one final canonical consumer."""

    for producer in WORKFLOW_PRODUCERS:
        if workflow.count(producer) != 1:
            raise AssertionError(f"release workflow must invoke `{producer}` once")
    final_check = "run: make vm-multicore-release-check"
    if workflow.count(final_check) != 1:
        raise AssertionError("release workflow omits canonical multicore closeout")
    if workflow.index(final_check) < workflow.index(
        "Download controlled multicore performance evidence"
    ):
        raise AssertionError("multicore closeout runs before evidence download")
    if workflow.index(final_check) > workflow.index(
        "Validate and seal complete AOT closeout"
    ):
        raise AssertionError("multicore closeout runs after parent AOT closeout")
    if "target/quality/vm-multicore-release-closeout.json" not in workflow:
        raise AssertionError("release workflow does not retain multicore closeout")


def validate_repository(root: Path = ROOT) -> None:
    """Validate the checked-out Makefile and release workflow."""

    validate_makefile((root / "Makefile").read_text(encoding="utf-8"))
    validate_workflow(
        (root / ".github/workflows/release.yml").read_text(encoding="utf-8")
    )


def file_sha256(path: Path) -> str:
    """Return one lowercase SHA-256 digest for an evidence file."""

    return hashlib.sha256(path.read_bytes()).hexdigest()


def bytes_sha256(value: bytes) -> str:
    """Return one lowercase SHA-256 digest for in-memory evidence."""

    return hashlib.sha256(value).hexdigest()


def load_json(path: Path) -> dict[str, object]:
    """Load one required JSON object."""

    value = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise AssertionError(f"release evidence `{path}` must be an object")
    return value


def source_revision(root: Path = ROOT) -> str:
    """Return the full checked-out Git source revision."""

    revision = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    if not mc9.is_revision(revision):
        raise AssertionError("checked-out source revision is not a full Git identity")
    return revision


def local_gate_graph_sha256() -> str:
    """Return the stable identity of the ordered local release gate graph."""

    return bytes_sha256(("\n".join(LOCAL_GATES) + "\n").encode("utf-8"))


def invariant_revision(inventory: bytes, contract: bytes) -> str:
    """Return the domain-separated revision of the runtime invariant contract."""

    digest = hashlib.sha256()
    digest.update(INVARIANT_REVISION_DOMAIN)
    digest.update(len(inventory).to_bytes(8, "big"))
    digest.update(inventory)
    digest.update(len(contract).to_bytes(8, "big"))
    digest.update(contract)
    return digest.hexdigest()


def validate_platform_report(
    report: dict[str, object],
    revision: str,
    joined_mc9: dict[str, object],
) -> None:
    """Require a complete same-run six-target platform aggregate."""

    expected = {
        "schema": platform_matrix.MATRIX_SCHEMA,
        "decision": "pass",
        "execution_environment": "github-actions",
        "repository": platform_matrix.OFFICIAL_REPOSITORY,
        "target_count": len(platform_matrix.TARGETS),
        "static_or_skipped_rows": 0,
        "workflow_ref": joined_mc9["workflow_ref"],
        "run_id": joined_mc9["run_id"],
        "run_attempt": joined_mc9["run_attempt"],
        "commit_sha": revision,
    }
    for field, value in expected.items():
        if report.get(field) != value:
            raise AssertionError(f"platform matrix has invalid `{field}`")
    platform_revision = report.get("source_revision")
    if (
        not isinstance(platform_revision, str)
        or not platform_revision
        or not revision.startswith(platform_revision)
    ):
        raise AssertionError("platform matrix belongs to another source revision")
    targets = report.get("targets")
    if not isinstance(targets, list) or len(targets) != len(platform_matrix.TARGETS):
        raise AssertionError("platform matrix does not contain every target")


def validate_invariants(inventory: dict[str, object]) -> None:
    """Require the canonical runtime invariant inventory identity."""

    if inventory.get("schema") != INVARIANT_SCHEMA:
        raise AssertionError("multicore invariant inventory has an invalid schema")
    if inventory.get("contract_document") != str(CONCURRENCY_CONTRACT.relative_to(ROOT)):
        raise AssertionError("multicore invariant inventory names another contract")
    upstream = inventory.get("upstream")
    if not isinstance(upstream, dict) or not mc9.is_revision(upstream.get("revision")):
        raise AssertionError("multicore invariant inventory has no pinned upstream revision")
    if upstream.get("product_dependency") is not False:
        raise AssertionError("multicore invariant inventory retained an OTP dependency")


def require_mapping(value: object, label: str) -> dict[str, object]:
    """Return one JSON object or reject the malformed field."""

    if not isinstance(value, dict):
        raise AssertionError(f"{label} must be an object")
    return value


def validate_closeout(report: dict[str, object]) -> None:
    """Validate the complete serialized multicore closeout contract."""

    expected = {
        "schema": SCHEMA,
        "decision": "pass",
        "repository": platform_matrix.OFFICIAL_REPOSITORY,
    }
    for field, value in expected.items():
        if report.get(field) != value:
            raise AssertionError(f"multicore closeout has invalid `{field}`")
    if not mc9.is_revision(report.get("source_revision")):
        raise AssertionError("multicore closeout has an invalid source revision")
    if not isinstance(report.get("workflow_ref"), str) or not report["workflow_ref"]:
        raise AssertionError("multicore closeout has no workflow reference")
    for field in ("run_id", "run_attempt"):
        if not isinstance(report.get(field), int) or isinstance(report.get(field), bool):
            raise AssertionError(f"multicore closeout has invalid `{field}`")

    gate_graph = require_mapping(report.get("local_gate_graph"), "local gate graph")
    if gate_graph.get("gates") != list(LOCAL_GATES):
        raise AssertionError("multicore closeout has a stale local gate graph")
    if gate_graph.get("sha256") != local_gate_graph_sha256():
        raise AssertionError("multicore closeout has a stale gate-graph digest")

    invariants = require_mapping(report.get("runtime_invariants"), "runtime invariants")
    for field in ("revision",):
        if not mc9.is_sha256(invariants.get(field)):
            raise AssertionError(f"runtime invariants have invalid `{field}`")
    for label in ("inventory", "contract"):
        value = require_mapping(invariants.get(label), f"runtime {label}")
        if not mc9.is_sha256(value.get("sha256")):
            raise AssertionError(f"runtime {label} has an invalid digest")

    evidence = require_mapping(report.get("evidence"), "release evidence")
    for label in ("mc9", "platform_matrix"):
        value = require_mapping(evidence.get(label), f"{label} evidence")
        if not mc9.is_sha256(value.get("sha256")):
            raise AssertionError(f"{label} evidence has an invalid digest")


def build_closeout(
    joined_mc9: dict[str, object],
    platform: dict[str, object],
    inventory: dict[str, object],
    inventory_bytes: bytes,
    contract_bytes: bytes,
    revision: str,
    mc9_sha256: str,
    platform_sha256: str,
) -> dict[str, object]:
    """Validate release evidence and return one revision-bound closeout."""

    mc9.validate_closeout(joined_mc9)
    if joined_mc9.get("source_revision") != revision:
        raise AssertionError("MC-9 evidence belongs to another source revision")
    validate_platform_report(platform, revision, joined_mc9)
    validate_invariants(inventory)
    for label, digest in (
        ("MC-9 evidence", mc9_sha256),
        ("platform matrix", platform_sha256),
    ):
        if not mc9.is_sha256(digest):
            raise AssertionError(f"{label} has an invalid digest")
    report = {
        "schema": SCHEMA,
        "decision": "pass",
        "source_revision": revision,
        "repository": joined_mc9["repository"],
        "workflow_ref": joined_mc9["workflow_ref"],
        "run_id": joined_mc9["run_id"],
        "run_attempt": joined_mc9["run_attempt"],
        "local_gate_graph": {
            "gates": list(LOCAL_GATES),
            "sha256": local_gate_graph_sha256(),
        },
        "runtime_invariants": {
            "revision": invariant_revision(inventory_bytes, contract_bytes),
            "inventory": {
                "path": str(INVARIANT_INVENTORY.relative_to(ROOT)),
                "schema": INVARIANT_SCHEMA,
                "sha256": bytes_sha256(inventory_bytes),
            },
            "contract": {
                "path": str(CONCURRENCY_CONTRACT.relative_to(ROOT)),
                "sha256": bytes_sha256(contract_bytes),
            },
        },
        "evidence": {
            "mc9": {
                "path": str(MC9_REPORT.relative_to(ROOT)),
                "schema": mc9.SCHEMA,
                "sha256": mc9_sha256,
            },
            "platform_matrix": {
                "path": str(PLATFORM_REPORT.relative_to(ROOT)),
                "schema": platform_matrix.MATRIX_SCHEMA,
                "sha256": platform_sha256,
            },
        },
    }
    validate_closeout(report)
    return report


def record(root: Path = ROOT) -> Path:
    """Validate canonical artifacts and write the multicore closeout report."""

    joined_mc9 = load_json(root / MC9_REPORT.relative_to(ROOT))
    platform = load_json(root / PLATFORM_REPORT.relative_to(ROOT))
    inventory_path = root / INVARIANT_INVENTORY.relative_to(ROOT)
    contract_path = root / CONCURRENCY_CONTRACT.relative_to(ROOT)
    inventory_bytes = inventory_path.read_bytes()
    contract_bytes = contract_path.read_bytes()
    inventory = json.loads(inventory_bytes)
    if not isinstance(inventory, dict):
        raise AssertionError("multicore invariant inventory must be an object")
    revision = source_revision(root)
    report = build_closeout(
        joined_mc9,
        platform,
        inventory,
        inventory_bytes,
        contract_bytes,
        revision,
        file_sha256(root / MC9_REPORT.relative_to(ROOT)),
        file_sha256(root / PLATFORM_REPORT.relative_to(ROOT)),
    )
    output = root / OUTPUT_REPORT.relative_to(ROOT)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"VM multicore release closeout passed: {revision}")
    return output


def require_rejection(action: Callable[[], object], message: str) -> None:
    """Require one adversarial contract mutation to be rejected."""

    try:
        action()
    except AssertionError:
        return
    raise AssertionError(message)


def self_test() -> None:
    """Exercise valid and intentionally corrupted release compositions."""

    makefile = (ROOT / "Makefile").read_text(encoding="utf-8")
    workflow = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
    validate_makefile(makefile)
    validate_workflow(workflow)

    require_rejection(
        lambda: validate_makefile(
            makefile.replace(
                "\tvm-multicore-memory-model-check \\\n",
                "",
                1,
            )
        ),
        "release contract accepted a missing semantic gate",
    )
    require_rejection(
        lambda: validate_makefile(
            makefile.replace(
                "\t$(MAKE) vm-multicore-mc9-evidence-check\n"
                "\t$(MAKE) $(VM_MULTICORE_RELEASE_LOCAL_GATES)\n",
                "\t$(MAKE) $(VM_MULTICORE_RELEASE_LOCAL_GATES)\n"
                "\t$(MAKE) vm-multicore-mc9-evidence-check\n",
                1,
            )
        ),
        "release contract accepted gates before official evidence",
    )
    require_rejection(
        lambda: validate_workflow(
            workflow.replace(
                "run: make vm-multicore-performance-check",
                "run: make vm-multicore-release-check",
                1,
            )
        ),
        "release contract accepted a missing controlled producer",
    )
    require_rejection(
        lambda: validate_workflow(
            workflow.replace(
                "run: make vm-multicore-release-check",
                "run: make vm-multicore-mc9-evidence-check",
                1,
            )
        ),
        "release contract accepted a missing canonical consumer",
    )

    revision = "a" * 40
    joined_mc9 = mc9.build_closeout(
        mc9.synthetic_performance(),
        mc9.synthetic_sanitizer(),
        "d" * 64,
        "e" * 64,
    )
    platform = {
        "schema": platform_matrix.MATRIX_SCHEMA,
        "decision": "pass",
        "source_revision": revision,
        "execution_environment": "github-actions",
        "repository": platform_matrix.OFFICIAL_REPOSITORY,
        "workflow_ref": joined_mc9["workflow_ref"],
        "run_id": joined_mc9["run_id"],
        "run_attempt": joined_mc9["run_attempt"],
        "commit_sha": revision,
        "target_count": len(platform_matrix.TARGETS),
        "targets": [{} for _ in platform_matrix.TARGETS],
        "static_or_skipped_rows": 0,
    }
    inventory_bytes = INVARIANT_INVENTORY.read_bytes()
    contract_bytes = CONCURRENCY_CONTRACT.read_bytes()
    inventory = json.loads(inventory_bytes)
    assert isinstance(inventory, dict)
    closeout = build_closeout(
        joined_mc9,
        platform,
        inventory,
        inventory_bytes,
        contract_bytes,
        revision,
        "f" * 64,
        "1" * 64,
    )
    if closeout.get("decision") != "pass":
        raise AssertionError("valid multicore release evidence did not pass")
    invalid_closeout = dict(closeout)
    invalid_closeout["local_gate_graph"] = {
        "gates": list(LOCAL_GATES),
        "sha256": "0" * 64,
    }
    require_rejection(
        lambda: validate_closeout(invalid_closeout),
        "release closeout accepted a stale gate-graph digest",
    )
    for field, value in (
        ("source_revision", "b" * 40),
        ("run_id", 8),
        ("static_or_skipped_rows", 1),
    ):
        invalid_platform = dict(platform)
        invalid_platform[field] = value
        require_rejection(
            lambda candidate=invalid_platform: build_closeout(
                joined_mc9,
                candidate,
                inventory,
                inventory_bytes,
                contract_bytes,
                revision,
                "f" * 64,
                "1" * 64,
            ),
            f"release closeout accepted invalid platform `{field}`",
        )
    invalid_mc9 = dict(joined_mc9)
    invalid_mc9["source_revision"] = "b" * 40
    require_rejection(
        lambda: build_closeout(
            invalid_mc9,
            platform,
            inventory,
            inventory_bytes,
            contract_bytes,
            revision,
            "f" * 64,
            "1" * 64,
        ),
        "release closeout accepted stale MC-9 evidence",
    )
    invalid_inventory = dict(inventory)
    invalid_inventory["schema"] = "stale"
    require_rejection(
        lambda: build_closeout(
            joined_mc9,
            platform,
            invalid_inventory,
            inventory_bytes,
            contract_bytes,
            revision,
            "f" * 64,
            "1" * 64,
        ),
        "release closeout accepted a stale invariant inventory",
    )
    if invariant_revision(inventory_bytes, contract_bytes) == invariant_revision(
        inventory_bytes,
        contract_bytes + b"\nchanged",
    ):
        raise AssertionError("runtime invariant revision ignored contract changes")
    print("VM multicore release composition self-test passed")


def main() -> int:
    """Dispatch repository validation or adversarial self-tests."""

    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("check", "record", "self-test"))
    command = parser.parse_args().command
    try:
        if command == "check":
            validate_repository()
        elif command == "record":
            record()
        else:
            self_test()
    except (
        AssertionError,
        json.JSONDecodeError,
        OSError,
        subprocess.CalledProcessError,
    ) as error:
        print(f"VM multicore release composition failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
