#!/usr/bin/env python3
"""Run and attest the Rust-instrumented AOT ThreadSanitizer lane."""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import sys
from pathlib import Path

import check_tvm_aot_platform_matrix as platform_matrix


ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "target/quality/tvm-aot-thread-sanitizer-report.json"
SCHEMA = "terlan.tvm-aot-thread-sanitizer.v1"
TARGET = "x86_64-unknown-linux-gnutsan"
TEST_FILTER = "runtime::vm::pure_native"


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
    """Return the exact checked-out commit identity."""

    return command_output(["git", "rev-parse", "HEAD"])


def validate_report(report: dict[str, object], require_ci: bool) -> None:
    """Reject incomplete, uninstrumented, stale, or non-CI attestations."""

    expected = {
        "schema": SCHEMA,
        "decision": "pass",
        "host": "linux-x86_64",
        "instrumented_target": TARGET,
        "test_filter": TEST_FILTER,
    }
    for field, value in expected.items():
        if report.get(field) != value:
            raise AssertionError(
                f"ThreadSanitizer report expected {field} `{value}`, found `{report.get(field)}`"
            )
    revision = report.get("source_revision")
    if not isinstance(revision, str) or len(revision) != 40:
        raise AssertionError("ThreadSanitizer report requires one full source revision")
    rustc = report.get("rustc")
    if not isinstance(rustc, str) or "rustc " not in rustc:
        raise AssertionError("ThreadSanitizer report requires Rust toolchain identity")
    if require_ci:
        if report.get("execution_environment") != "github-actions":
            raise AssertionError("release ThreadSanitizer evidence must come from GitHub Actions")
        if report.get("repository") != platform_matrix.OFFICIAL_REPOSITORY:
            raise AssertionError("ThreadSanitizer evidence belongs to the wrong repository")
        if report.get("commit_sha") != revision:
            raise AssertionError("ThreadSanitizer commit does not match its source revision")
        if not isinstance(report.get("run_id"), int) or not isinstance(
            report.get("run_attempt"), int
        ):
            raise AssertionError("ThreadSanitizer CI run identity is incomplete")


def run_instrumented_tests() -> Path:
    """Run the fully instrumented runtime test family and write its attestation."""

    machine = platform.machine().lower()
    normalized_machine = "x86_64" if machine == "amd64" else machine
    if (platform.system().lower(), normalized_machine) != ("linux", "x86_64"):
        raise AssertionError("ThreadSanitizer validation requires Linux x86-64")
    installed = set(command_output(["rustup", "target", "list", "--installed"]).splitlines())
    if TARGET not in installed:
        raise AssertionError(f"install the Rust `{TARGET}` target before validation")

    subprocess.run(
        [
            "cargo",
            "test",
            "--locked",
            "-p",
            "terlan",
            "--bin",
            "terlan-vm",
            "--target",
            TARGET,
            TEST_FILTER,
        ],
        cwd=ROOT,
        check=True,
        env={**os.environ, "TSAN_OPTIONS": "halt_on_error=1"},
    )
    revision = source_revision()
    report: dict[str, object] = {
        "schema": SCHEMA,
        "decision": "pass",
        "host": "linux-x86_64",
        "instrumented_target": TARGET,
        "test_filter": TEST_FILTER,
        "source_revision": revision,
        "rustc": command_output(["rustc", "--version", "--verbose"]),
        **platform_matrix.execution_provenance(revision),
    }
    validate_report(
        report,
        require_ci=os.environ.get("GITHUB_ACTIONS", "").lower() == "true",
    )
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"TVM AOT ThreadSanitizer passed: {TARGET}")
    return REPORT


def self_test() -> None:
    """Prove the attestation rejects stale identity and uninstrumented evidence."""

    valid: dict[str, object] = {
        "schema": SCHEMA,
        "decision": "pass",
        "host": "linux-x86_64",
        "instrumented_target": TARGET,
        "test_filter": TEST_FILTER,
        "source_revision": "a" * 40,
        "rustc": "rustc 1.96.0\nbinary: rustc",
        "execution_environment": "github-actions",
        "repository": platform_matrix.OFFICIAL_REPOSITORY,
        "workflow_ref": "terlan-lang/terlan/.github/workflows/ci.yml@refs/heads/main",
        "run_id": 7,
        "run_attempt": 1,
        "commit_sha": "a" * 40,
    }
    validate_report(valid, require_ci=True)
    for field, value in (
        ("instrumented_target", "x86_64-unknown-linux-gnu"),
        ("decision", "skipped"),
        ("repository", "fork/terlan"),
        ("commit_sha", "b" * 40),
        ("run_id", "7"),
    ):
        invalid = dict(valid)
        invalid[field] = value
        try:
            validate_report(invalid, require_ci=True)
        except AssertionError:
            pass
        else:
            raise AssertionError(f"ThreadSanitizer report accepted invalid `{field}`")
    print("TVM AOT ThreadSanitizer self-test passed")


def main() -> int:
    """Dispatch the executable ThreadSanitizer contract."""

    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("run", "self-test"))
    command = parser.parse_args().command
    try:
        if command == "run":
            run_instrumented_tests()
        else:
            self_test()
    except (AssertionError, OSError, subprocess.CalledProcessError) as error:
        print(f"TVM AOT ThreadSanitizer failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
