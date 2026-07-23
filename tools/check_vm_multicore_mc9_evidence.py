#!/usr/bin/env python3
"""Join enforced performance and sanitizer evidence for MC-9 closeout."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

import check_tvm_aot_platform_matrix as platform_matrix
import check_vm_multicore_thread_sanitizer as sanitizer


ROOT = Path(__file__).resolve().parents[1]
PERFORMANCE_REPORT = ROOT / "target/quality/vm-multicore-performance.json"
SANITIZER_REPORT = ROOT / "target/quality/vm-multicore-thread-sanitizer-report.json"
OUTPUT_REPORT = ROOT / "target/quality/vm-multicore-mc9-evidence.json"
SCHEMA = "terlan.vm-multicore-mc9-evidence.v1"
PERFORMANCE_SCHEMA = "terlan.vm-multicore-performance.v1"
PERFORMANCE_POLICY_SCHEMA = "terlan-vm-multicore-performance-limits-v1"
DEDICATED_RUNNER = "terlan-linux-x86_64-multicore-v1"
PERFORMANCE_TOOLCHAIN = "1.96.0"
EXPECTED_WIDTHS = {1, 2, 4}
EXPECTED_MIXED_METRICS = {
    "scheduler_wait",
    "mailbox_delivery",
    "timer_delay",
    "http_latency",
    "failed_steal_backoff",
    "allocation_pause",
    "collection_pause",
}


def is_sha256(value: object) -> bool:
    """Return whether a value is one canonical lowercase SHA-256 digest."""

    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def is_revision(value: object) -> bool:
    """Return whether a value is one full lowercase Git revision."""

    return (
        isinstance(value, str)
        and len(value) == 40
        and all(character in "0123456789abcdef" for character in value)
    )


def file_sha256(path: Path) -> str:
    """Return one lowercase SHA-256 digest for an evidence file."""

    return hashlib.sha256(path.read_bytes()).hexdigest()


def require_mapping(value: object, label: str) -> dict[str, object]:
    """Return a JSON object or reject the malformed field."""

    if not isinstance(value, dict):
        raise AssertionError(f"{label} must be an object")
    return value


def require_list(value: object, label: str) -> list[object]:
    """Return a JSON array or reject the malformed field."""

    if not isinstance(value, list):
        raise AssertionError(f"{label} must be an array")
    return value


def require_minimum(value: object, minimum: float, label: str) -> None:
    """Require one finite numeric value to meet a lower bound."""

    if (
        not isinstance(value, (int, float))
        or isinstance(value, bool)
        or not float(value) >= minimum
    ):
        raise AssertionError(f"{label} must be at least {minimum}")


def validate_performance_report(report: dict[str, object]) -> None:
    """Reject record-only, unqualified, stale, or incomplete performance data."""

    expected = {
        "schema": PERFORMANCE_SCHEMA,
        "target_os": "linux",
        "target_arch": "x86_64",
        "optimization_profile": "release",
        "eligible_for_parallel_assertion": True,
    }
    for field, value in expected.items():
        if report.get(field) != value:
            raise AssertionError(
                f"MC-9 performance expected {field} `{value}`, found `{report.get(field)}`"
            )
    revision = report.get("source_revision")
    if not is_revision(revision):
        raise AssertionError("MC-9 performance requires one full source revision")
    rustc = report.get("rustc_version")
    if not isinstance(rustc, str) or not rustc.startswith(
        f"rustc {PERFORMANCE_TOOLCHAIN} "
    ):
        raise AssertionError("MC-9 performance used an unpinned Rust compiler")
    for field in (
        "workload_sha256",
        "native_image_sha256",
        "runtime_workload_contract_sha256",
        "mixed_tail_contract_sha256",
        "performance_policy_sha256",
        "benchmark_sha256",
    ):
        if not is_sha256(report.get(field)):
            raise AssertionError(f"MC-9 performance has invalid `{field}`")

    provenance = require_mapping(report.get("provenance"), "performance provenance")
    provenance_expected = {
        "execution_environment": "github-actions",
        "source_tree_clean": True,
        "repository": platform_matrix.OFFICIAL_REPOSITORY,
        "commit_sha": revision,
        "runner_environment": "self-hosted",
    }
    for field, value in provenance_expected.items():
        if provenance.get(field) != value:
            raise AssertionError(
                f"MC-9 performance provenance expected {field} `{value}`"
            )
    for field in ("workflow_ref", "runner_name"):
        if not isinstance(provenance.get(field), str) or not provenance[field]:
            raise AssertionError(f"MC-9 performance provenance requires `{field}`")
    for field in ("run_id", "run_attempt"):
        if not isinstance(provenance.get(field), int) or isinstance(
            provenance.get(field), bool
        ):
            raise AssertionError(f"MC-9 performance provenance requires numeric `{field}`")

    background = require_mapping(report.get("background_load"), "background load")
    if background.get("declared_state") != "controlled":
        raise AssertionError("MC-9 performance requires controlled background load")
    hardware = require_mapping(report.get("hardware"), "performance hardware")
    require_minimum(
        hardware.get("effective_parallelism"),
        2,
        "effective performance CPUs",
    )

    policy = require_mapping(report.get("performance_policy"), "performance policy")
    policy_expected = {
        "schema": PERFORMANCE_POLICY_SCHEMA,
        "dedicated_runner_label": DEDICATED_RUNNER,
        "requested_runner_label": DEDICATED_RUNNER,
        "enforced": True,
        "status": "passed",
        "record_only_reason": None,
    }
    for field, value in policy_expected.items():
        if policy.get(field) != value:
            raise AssertionError(
                f"MC-9 performance policy expected {field} `{value}`"
            )
    require_minimum(
        policy.get("observed_two_scheduler_median_speedup"),
        1.5,
        "two-scheduler median speedup",
    )
    require_minimum(
        policy.get("observed_two_scheduler_confidence_lower_bound"),
        1.25,
        "two-scheduler confidence lower bound",
    )
    mixed_policy = require_list(policy.get("mixed_tail"), "mixed-tail policy")
    if {require_mapping(row, "mixed-tail policy row").get("metric") for row in mixed_policy} != (
        EXPECTED_MIXED_METRICS
    ):
        raise AssertionError("MC-9 performance policy has incomplete mixed-tail metrics")
    for row in mixed_policy:
        evidence = require_mapping(row, "mixed-tail policy row")
        for field in ("p95_ratio", "p99_ratio"):
            value = evidence.get(field)
            if (
                not isinstance(value, (int, float))
                or isinstance(value, bool)
                or not 0.0 <= float(value) <= 2.0
            ):
                raise AssertionError(f"MC-9 performance `{field}` exceeds policy")

    measurements = require_list(report.get("measurements"), "scheduler measurements")
    widths = {
        require_mapping(row, "scheduler measurement").get("requested_schedulers")
        for row in measurements
    }
    if widths != EXPECTED_WIDTHS:
        raise AssertionError("MC-9 performance does not cover widths one, two, and four")
    width_two = next(
        require_mapping(row, "scheduler measurement")
        for row in measurements
        if require_mapping(row, "scheduler measurement").get("requested_schedulers") == 2
    )
    if width_two.get("overlap_proven") is not True:
        raise AssertionError("MC-9 performance lacks two-scheduler overlap")
    require_minimum(
        width_two.get("maximum_simultaneously_active_schedulers"),
        2,
        "simultaneously active schedulers",
    )
    mixed_tail = require_mapping(report.get("mixed_load_tail"), "mixed-load tail")
    if mixed_tail.get("cpu_overlap_proven") is not True:
        raise AssertionError("MC-9 mixed-load performance lacks CPU overlap")
    require_minimum(
        mixed_tail.get("maximum_simultaneously_active_schedulers"),
        2,
        "mixed-load active schedulers",
    )


def build_closeout(
    performance: dict[str, object],
    sanitizer_report: dict[str, object],
    performance_sha256: str,
    sanitizer_sha256: str,
) -> dict[str, object]:
    """Validate and join two reports from the same official workflow run."""

    validate_performance_report(performance)
    sanitizer.validate_report(sanitizer_report, require_ci=True)
    revision = performance["source_revision"]
    if sanitizer_report.get("source_revision") != revision:
        raise AssertionError("MC-9 reports describe different source revisions")
    provenance = require_mapping(performance["provenance"], "performance provenance")
    joins = (
        ("repository", provenance.get("repository"), sanitizer_report.get("repository")),
        ("workflow_ref", provenance.get("workflow_ref"), sanitizer_report.get("workflow_ref")),
        ("run_id", provenance.get("run_id"), sanitizer_report.get("run_id")),
        ("run_attempt", provenance.get("run_attempt"), sanitizer_report.get("run_attempt")),
        ("commit_sha", provenance.get("commit_sha"), sanitizer_report.get("commit_sha")),
    )
    for field, performance_value, sanitizer_value in joins:
        if performance_value != sanitizer_value:
            raise AssertionError(f"MC-9 reports came from different `{field}` values")
    if not is_sha256(performance_sha256) or not is_sha256(sanitizer_sha256):
        raise AssertionError("MC-9 report digest is malformed")
    return {
        "schema": SCHEMA,
        "decision": "pass",
        "source_revision": revision,
        "repository": provenance["repository"],
        "workflow_ref": provenance["workflow_ref"],
        "run_id": provenance["run_id"],
        "run_attempt": provenance["run_attempt"],
        "dedicated_runner_label": DEDICATED_RUNNER,
        "performance_runner_name": provenance["runner_name"],
        "performance_report_sha256": performance_sha256,
        "sanitizer_toolchain": sanitizer.TOOLCHAIN,
        "sanitizer_target": sanitizer.TARGET,
        "sanitizer_report_sha256": sanitizer_sha256,
    }


def validate_closeout(report: dict[str, object]) -> None:
    """Validate one sealed MC-9 report for downstream release composition."""

    expected = {
        "schema": SCHEMA,
        "decision": "pass",
        "repository": platform_matrix.OFFICIAL_REPOSITORY,
        "dedicated_runner_label": DEDICATED_RUNNER,
        "sanitizer_toolchain": sanitizer.TOOLCHAIN,
        "sanitizer_target": sanitizer.TARGET,
    }
    for field, value in expected.items():
        if report.get(field) != value:
            raise AssertionError(f"MC-9 closeout has invalid `{field}`")
    if not is_revision(report.get("source_revision")):
        raise AssertionError("MC-9 closeout has an invalid source revision")
    for field in ("performance_report_sha256", "sanitizer_report_sha256"):
        if not is_sha256(report.get(field)):
            raise AssertionError(f"MC-9 closeout has invalid `{field}`")
    if not isinstance(report.get("workflow_ref"), str) or not report["workflow_ref"]:
        raise AssertionError("MC-9 closeout has no workflow reference")
    if not isinstance(report.get("performance_runner_name"), str) or not report[
        "performance_runner_name"
    ]:
        raise AssertionError("MC-9 closeout has no performance runner")
    for field in ("run_id", "run_attempt"):
        if not isinstance(report.get(field), int) or isinstance(report.get(field), bool):
            raise AssertionError(f"MC-9 closeout has invalid `{field}`")


def load_report(path: Path) -> dict[str, object]:
    """Load one required JSON evidence object."""

    report = json.loads(path.read_text(encoding="utf-8"))
    return require_mapping(report, str(path))


def seal() -> Path:
    """Join canonical performance and sanitizer reports into MC-9 evidence."""

    performance = load_report(PERFORMANCE_REPORT)
    sanitizer_report = load_report(SANITIZER_REPORT)
    closeout = build_closeout(
        performance,
        sanitizer_report,
        file_sha256(PERFORMANCE_REPORT),
        file_sha256(SANITIZER_REPORT),
    )
    validate_closeout(closeout)
    OUTPUT_REPORT.parent.mkdir(parents=True, exist_ok=True)
    OUTPUT_REPORT.write_text(
        json.dumps(closeout, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(f"VM multicore MC-9 evidence passed: {closeout['source_revision']}")
    return OUTPUT_REPORT


def synthetic_performance() -> dict[str, object]:
    """Return the smallest complete performance report used by self-tests."""

    revision = "a" * 40
    return {
        "schema": PERFORMANCE_SCHEMA,
        "source_revision": revision,
        "rustc_version": f"rustc {PERFORMANCE_TOOLCHAIN} (pinned)",
        "target_os": "linux",
        "target_arch": "x86_64",
        "optimization_profile": "release",
        "eligible_for_parallel_assertion": True,
        "background_load": {"declared_state": "controlled"},
        "hardware": {"effective_parallelism": 4},
        "provenance": {
            "execution_environment": "github-actions",
            "source_tree_clean": True,
            "repository": platform_matrix.OFFICIAL_REPOSITORY,
            "workflow_ref": "terlan-lang/terlan/.github/workflows/release.yml@refs/tags/v0.0.7",
            "run_id": 7,
            "run_attempt": 1,
            "commit_sha": revision,
            "runner_name": "performance-1",
            "runner_environment": "self-hosted",
        },
        **{
            field: "b" * 64
            for field in (
                "workload_sha256",
                "native_image_sha256",
                "runtime_workload_contract_sha256",
                "mixed_tail_contract_sha256",
                "performance_policy_sha256",
                "benchmark_sha256",
            )
        },
        "performance_policy": {
            "schema": PERFORMANCE_POLICY_SCHEMA,
            "dedicated_runner_label": DEDICATED_RUNNER,
            "requested_runner_label": DEDICATED_RUNNER,
            "enforced": True,
            "status": "passed",
            "record_only_reason": None,
            "observed_two_scheduler_median_speedup": 1.75,
            "observed_two_scheduler_confidence_lower_bound": 1.5,
            "mixed_tail": [
                {"metric": metric, "p95_ratio": 1.0, "p99_ratio": 1.0}
                for metric in sorted(EXPECTED_MIXED_METRICS)
            ],
        },
        "measurements": [
            {
                "requested_schedulers": width,
                "overlap_proven": width != 2 or True,
                "maximum_simultaneously_active_schedulers": width,
            }
            for width in sorted(EXPECTED_WIDTHS)
        ],
        "mixed_load_tail": {
            "cpu_overlap_proven": True,
            "maximum_simultaneously_active_schedulers": 2,
        },
    }


def synthetic_sanitizer() -> dict[str, object]:
    """Return complete pinned sanitizer evidence used by self-tests."""

    revision = "a" * 40
    return {
        "schema": sanitizer.SCHEMA,
        "decision": "pass",
        "host": "linux-x86_64",
        "toolchain": sanitizer.TOOLCHAIN,
        "rustc": f"rustc {sanitizer.TOOLCHAIN} (pinned)\nbinary: rustc",
        "instrumented_target": sanitizer.TARGET,
        "test_name": sanitizer.TEST_NAME,
        "seed_count": sanitizer.SEED_COUNT,
        "stress_report_sha256": "c" * 64,
        "source_revision": revision,
        "source_tree_clean": True,
        "execution_environment": "github-actions",
        "repository": platform_matrix.OFFICIAL_REPOSITORY,
        "workflow_ref": "terlan-lang/terlan/.github/workflows/release.yml@refs/tags/v0.0.7",
        "run_id": 7,
        "run_attempt": 1,
        "commit_sha": revision,
    }


def validate_contract_files() -> None:
    """Require Make and release workflows to retain the complete MC-9 join."""

    makefile = (ROOT / "Makefile").read_text(encoding="utf-8")
    for fragment in (
        "vm-multicore-mc9-evidence-contract-check:",
        "vm-multicore-mc9-evidence-check:",
        "tools/check_vm_multicore_mc9_evidence.py seal",
        "$(MAKE) vm-multicore-mc9-evidence-check",
    ):
        if fragment not in makefile:
            raise AssertionError(f"Makefile omits `{fragment}`")
    release = (ROOT / ".github/workflows/release.yml").read_text(encoding="utf-8")
    for fragment in (
        "runs-on: [self-hosted, linux, x64, terlan-linux-x86_64-multicore-v1]",
        "TERLAN_VM_MULTICORE_DEDICATED_RUNNER: terlan-linux-x86_64-multicore-v1",
        "TERLAN_BENCH_BACKGROUND_LOAD: controlled",
        "make vm-multicore-release-check",
        "vm-multicore-mc9-evidence.json",
    ):
        if fragment not in release:
            raise AssertionError(f"release.yml omits `{fragment}`")


def expect_rejected(
    performance: dict[str, object],
    sanitizer_report: dict[str, object],
    label: str,
) -> None:
    """Require one malformed synthetic evidence pair to fail."""

    try:
        build_closeout(performance, sanitizer_report, "d" * 64, "e" * 64)
    except AssertionError:
        return
    raise AssertionError(f"MC-9 evidence accepted invalid `{label}`")


def self_test() -> None:
    """Prove record-only, cross-run, stale, and hosted evidence fail closed."""

    performance = synthetic_performance()
    sanitizer_report = synthetic_sanitizer()
    closeout = build_closeout(performance, sanitizer_report, "d" * 64, "e" * 64)
    validate_closeout(closeout)
    if closeout.get("decision") != "pass":
        raise AssertionError("valid MC-9 evidence did not pass")
    invalid_closeout = dict(closeout)
    invalid_closeout["run_attempt"] = False
    try:
        validate_closeout(invalid_closeout)
    except AssertionError:
        pass
    else:
        raise AssertionError("MC-9 closeout accepted a boolean run attempt")

    for field, value in (
        ("eligible_for_parallel_assertion", False),
        ("rustc_version", "rustc stable"),
        ("source_revision", "f" * 40),
    ):
        invalid = dict(performance)
        invalid[field] = value
        expect_rejected(invalid, sanitizer_report, field)
    for field, value in (
        ("enforced", False),
        ("status", "record_only"),
        ("requested_runner_label", None),
    ):
        invalid = dict(performance)
        invalid["performance_policy"] = {
            **require_mapping(performance["performance_policy"], "policy"),
            field: value,
        }
        expect_rejected(invalid, sanitizer_report, field)
    for field, value in (
        ("source_tree_clean", False),
        ("runner_environment", "github-hosted"),
        ("run_id", 8),
        ("run_attempt", False),
        ("commit_sha", "f" * 40),
    ):
        invalid = dict(performance)
        invalid["provenance"] = {
            **require_mapping(performance["provenance"], "provenance"),
            field: value,
        }
        expect_rejected(invalid, sanitizer_report, field)
    for field, value in (
        ("source_tree_clean", False),
        ("source_revision", "f" * 40),
        ("run_id", 8),
        ("toolchain", "stable"),
    ):
        invalid_sanitizer = dict(sanitizer_report)
        invalid_sanitizer[field] = value
        expect_rejected(performance, invalid_sanitizer, f"sanitizer {field}")
    validate_contract_files()
    print("VM multicore MC-9 evidence self-test passed")


def main() -> int:
    """Dispatch the MC-9 evidence contract."""

    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("seal", "self-test"))
    command = parser.parse_args().command
    try:
        if command == "seal":
            seal()
        else:
            self_test()
    except (AssertionError, json.JSONDecodeError, OSError) as error:
        print(f"VM multicore MC-9 evidence failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
