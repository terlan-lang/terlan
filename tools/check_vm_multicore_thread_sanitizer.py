#!/usr/bin/env python3
"""Run and attest the pinned multicore VM ThreadSanitizer lane."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import subprocess
import sys
from pathlib import Path

import check_tvm_aot_platform_matrix as platform_matrix


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "target/quality/vm-multicore-thread-sanitizer-report.json"
STRESS_REPORT = ROOT / "target/quality/vm-multicore-memory-model-tsan.json"
SCHEMA = "terlan.vm-multicore-thread-sanitizer.v1"
STRESS_SCHEMA = "terlan.vm-multicore-memory-model.v1"
TOOLCHAIN = "1.96.0"
TARGET = "x86_64-unknown-linux-gnutsan"
TEST_NAME = (
    "runtime::vm::fixed_scheduler_control::fixed_scheduler_control_stress_test::"
    "bounded_seeded_multicore_memory_model_has_deadlock_watchdog"
)
SEED_COUNT = 8


def command_output(command: list[str]) -> str:
    """Run one read-only command and return normalized standard output."""

    return subprocess.run(
        command,
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()


def source_revision() -> str:
    """Return the exact checked-out source revision."""

    return command_output(["git", "rev-parse", "HEAD"])


def file_sha256(path: Path) -> str:
    """Return one lowercase SHA-256 digest for the requested evidence file."""

    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_stress_report(report: dict[str, object]) -> None:
    """Reject incomplete portable stress evidence."""

    if report.get("schema") != STRESS_SCHEMA or report.get("decision") != "pass":
        raise AssertionError("ThreadSanitizer stress evidence did not pass")
    seeds = report.get("seeds")
    if not isinstance(seeds, list) or len(seeds) != SEED_COUNT:
        raise AssertionError(
            f"ThreadSanitizer stress evidence requires {SEED_COUNT} seeds"
        )
    if len(set(seeds)) != SEED_COUNT:
        raise AssertionError("ThreadSanitizer stress evidence repeats a seed")
    if report.get("watchdog_timeout_millis") != 15_000:
        raise AssertionError("ThreadSanitizer stress evidence lost its watchdog")


def validate_report(report: dict[str, object], require_ci: bool) -> None:
    """Reject stale, skipped, unpinned, or uninstrumented attestations."""

    expected = {
        "schema": SCHEMA,
        "decision": "pass",
        "host": "linux-x86_64",
        "toolchain": TOOLCHAIN,
        "instrumented_target": TARGET,
        "test_name": TEST_NAME,
        "seed_count": SEED_COUNT,
    }
    for field, value in expected.items():
        if report.get(field) != value:
            raise AssertionError(
                f"multicore ThreadSanitizer expected {field} `{value}`, "
                f"found `{report.get(field)}`"
            )
    revision = report.get("source_revision")
    if not isinstance(revision, str) or len(revision) != 40:
        raise AssertionError("multicore ThreadSanitizer requires one full revision")
    rustc = report.get("rustc")
    if not isinstance(rustc, str) or not rustc.startswith(f"rustc {TOOLCHAIN} "):
        raise AssertionError("multicore ThreadSanitizer used an unpinned compiler")
    digest = report.get("stress_report_sha256")
    if (
        not isinstance(digest, str)
        or len(digest) != 64
        or any(character not in "0123456789abcdef" for character in digest)
    ):
        raise AssertionError("multicore ThreadSanitizer stress digest is invalid")
    if require_ci:
        if report.get("execution_environment") != "github-actions":
            raise AssertionError("release sanitizer evidence must come from GitHub Actions")
        if report.get("repository") != platform_matrix.OFFICIAL_REPOSITORY:
            raise AssertionError("multicore sanitizer evidence belongs to another repository")
        if report.get("commit_sha") != revision:
            raise AssertionError("multicore sanitizer commit does not match its source")
        if (
            not isinstance(report.get("run_id"), int)
            or isinstance(report.get("run_id"), bool)
            or not isinstance(report.get("run_attempt"), int)
            or isinstance(report.get("run_attempt"), bool)
        ):
            raise AssertionError("multicore sanitizer CI identity is incomplete")


def validate_contract_files() -> None:
    """Require CI, release, and Make to retain the pinned sanitizer lane."""

    makefile = (ROOT / "Makefile").read_text(encoding="utf-8")
    for fragment in (
        "vm-multicore-memory-model-check:",
        "vm-multicore-thread-sanitizer-check:",
        "tools/check_vm_multicore_thread_sanitizer.py run",
    ):
        if fragment not in makefile:
            raise AssertionError(f"Makefile omits `{fragment}`")
    for workflow_name in ("ci.yml", "release.yml"):
        workflow = (ROOT / ".github/workflows" / workflow_name).read_text(
            encoding="utf-8"
        )
        for fragment in (
            f"rustup toolchain install {TOOLCHAIN} --profile minimal --target {TARGET}",
            "make vm-multicore-thread-sanitizer-check",
            "vm-multicore-thread-sanitizer-report.json",
        ):
            if fragment not in workflow:
                raise AssertionError(f"{workflow_name} omits `{fragment}`")


def run_instrumented_tests() -> Path:
    """Run every seeded child under the pinned ThreadSanitizer runtime."""

    machine = platform.machine().lower()
    normalized_machine = "x86_64" if machine == "amd64" else machine
    if (platform.system().lower(), normalized_machine) != ("linux", "x86_64"):
        raise AssertionError("multicore ThreadSanitizer requires Linux x86-64")
    installed = set(
        command_output(
            [
                "rustup",
                "target",
                "list",
                "--installed",
                "--toolchain",
                TOOLCHAIN,
            ]
        ).splitlines()
    )
    if TARGET not in installed:
        raise AssertionError(
            f"install `{TARGET}` for Rust {TOOLCHAIN} before validation"
        )
    rustc = command_output(["rustc", f"+{TOOLCHAIN}", "--version", "--verbose"])
    if not rustc.startswith(f"rustc {TOOLCHAIN} "):
        raise AssertionError(f"expected Rust {TOOLCHAIN}, found `{rustc.splitlines()[0]}`")

    STRESS_REPORT.unlink(missing_ok=True)
    subprocess.run(
        [
            "cargo",
            f"+{TOOLCHAIN}",
            "test",
            "--locked",
            "-p",
            "terlan",
            "--bin",
            "terlan-vm",
            "--target",
            TARGET,
            TEST_NAME,
            "--",
            "--exact",
            "--nocapture",
        ],
        cwd=ROOT,
        check=True,
        timeout=600,
        env={
            **os.environ,
            "TERLAN_VM_MULTICORE_STRESS_OUTPUT": str(STRESS_REPORT),
            "TSAN_OPTIONS": "halt_on_error=1 exitcode=66",
        },
    )
    stress = json.loads(STRESS_REPORT.read_text(encoding="utf-8"))
    validate_stress_report(stress)
    revision = source_revision()
    report: dict[str, object] = {
        "schema": SCHEMA,
        "decision": "pass",
        "host": "linux-x86_64",
        "toolchain": TOOLCHAIN,
        "rustc": rustc,
        "instrumented_target": TARGET,
        "test_name": TEST_NAME,
        "seed_count": SEED_COUNT,
        "stress_report_sha256": file_sha256(STRESS_REPORT),
        "source_revision": revision,
        **platform_matrix.execution_provenance(revision),
    }
    validate_report(
        report,
        require_ci=os.environ.get("GITHUB_ACTIONS", "").lower() == "true",
    )
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(
        json.dumps(report, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    print(f"VM multicore ThreadSanitizer passed: Rust {TOOLCHAIN} {TARGET}")
    return REPORT


def self_test() -> None:
    """Prove malformed stress, toolchain, target, and CI evidence fail closed."""

    stress: dict[str, object] = {
        "schema": STRESS_SCHEMA,
        "decision": "pass",
        "seeds": [f"0x{seed:016x}" for seed in range(SEED_COUNT)],
        "watchdog_timeout_millis": 15_000,
    }
    validate_stress_report(stress)
    invalid_stress = dict(stress)
    invalid_stress["seeds"] = ["0x0000000000000000"] * SEED_COUNT
    try:
        validate_stress_report(invalid_stress)
    except AssertionError:
        pass
    else:
        raise AssertionError("multicore sanitizer accepted repeated stress seeds")

    valid: dict[str, object] = {
        "schema": SCHEMA,
        "decision": "pass",
        "host": "linux-x86_64",
        "toolchain": TOOLCHAIN,
        "rustc": f"rustc {TOOLCHAIN} (pinned)\nbinary: rustc",
        "instrumented_target": TARGET,
        "test_name": TEST_NAME,
        "seed_count": SEED_COUNT,
        "stress_report_sha256": "a" * 64,
        "source_revision": "b" * 40,
        "execution_environment": "github-actions",
        "repository": platform_matrix.OFFICIAL_REPOSITORY,
        "workflow_ref": "terlan-lang/terlan/.github/workflows/ci.yml@refs/heads/main",
        "run_id": 7,
        "run_attempt": 1,
        "commit_sha": "b" * 40,
    }
    validate_report(valid, require_ci=True)
    for field, value in (
        ("toolchain", "stable"),
        ("instrumented_target", "x86_64-unknown-linux-gnu"),
        ("decision", "skipped"),
        ("seed_count", 0),
        ("repository", "fork/terlan"),
        ("commit_sha", "c" * 40),
        ("run_id", "7"),
        ("run_attempt", False),
    ):
        invalid = dict(valid)
        invalid[field] = value
        try:
            validate_report(invalid, require_ci=True)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"multicore sanitizer accepted invalid `{field}`")
    validate_contract_files()
    print("VM multicore ThreadSanitizer self-test passed")


def main() -> int:
    """Dispatch the executable multicore ThreadSanitizer contract."""

    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("run", "self-test"))
    command = parser.parse_args().command
    try:
        if command == "run":
            run_instrumented_tests()
        else:
            self_test()
    except (
        AssertionError,
        json.JSONDecodeError,
        OSError,
        subprocess.CalledProcessError,
        subprocess.TimeoutExpired,
    ) as error:
        print(f"VM multicore ThreadSanitizer failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
