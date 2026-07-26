#!/usr/bin/env python3
"""Produce and aggregate executable TVM AOT platform attestations."""

from __future__ import annotations

import argparse
import json
import os
import platform
import subprocess
import sys
import tempfile
from pathlib import Path
from unittest.mock import patch

import check_tvm_package_install_consumer as package_consumer


ROOT = Path(__file__).resolve().parents[1]
TARGET_REPORT_DIR = ROOT / "target/quality/tvm-aot-platform"
MATRIX_REPORT = ROOT / "target/quality/tvm-aot-platform-matrix-report.json"
OFFICIAL_REPOSITORY = "terlan-lang/terlan"
TARGET_SCHEMA = "terlan.tvm-aot-platform-target.v2"
MATRIX_SCHEMA = "terlan.tvm-aot-platform-matrix.v2"
REQUIRED_EXECUTED_CHECKS = (
    "compiler-generated-image",
    "archive-package-admission",
    "installed-package-admission",
    "release-binary-smoke",
    "public-installer-smoke",
    "native-debug-stack-metadata",
    "deterministic-support-bundle",
    "crash-metadata",
    "hot-reload-replacement",
    "generation-quarantine",
    "incompatible-image-rejection",
)
TARGETS = {
    "linux-x86_64": {
        "os": "linux",
        "arch": "x86_64",
        "target_triple": "x86_64-unknown-linux-gnu",
        "object_format": "elf",
        "operating_system": "linux",
        "calling_convention": "system_v",
    },
    "linux-aarch64": {
        "os": "linux",
        "arch": "aarch64",
        "target_triple": "aarch64-unknown-linux-gnu",
        "object_format": "elf",
        "operating_system": "linux",
        "calling_convention": "system_v",
    },
    "macos-x86_64": {
        "os": "macos",
        "arch": "x86_64",
        "target_triple": "x86_64-apple-darwin",
        "object_format": "mach-o",
        "operating_system": "darwin",
        "calling_convention": "system_v",
    },
    "macos-aarch64": {
        "os": "macos",
        "arch": "aarch64",
        "target_triple": "aarch64-apple-darwin",
        "object_format": "mach-o",
        "operating_system": "darwin",
        "calling_convention": "apple_aarch64",
    },
    "windows-x86_64": {
        "os": "windows",
        "arch": "x86_64",
        "target_triple": "x86_64-pc-windows-msvc",
        "object_format": "pe",
        "operating_system": "windows",
        "calling_convention": "windows_fastcall",
    },
    "windows-aarch64": {
        "os": "windows",
        "arch": "aarch64",
        "target_triple": "aarch64-pc-windows-msvc",
        "object_format": "pe",
        "operating_system": "windows",
        "calling_convention": "windows_fastcall",
    },
}
RUNNERS = {
    "linux-x86_64": "ubuntu-24.04",
    "linux-aarch64": "ubuntu-24.04-arm",
    "macos-x86_64": "macos-15-intel",
    "macos-aarch64": "macos-15",
    "windows-x86_64": "windows-2025",
    "windows-aarch64": "windows-11-arm",
}
RELEASE_EVIDENCE = (
    "tvm-aot-release-closeout-report.json",
    "tvm-aot-release-clean-checkout.json",
    "tvm-aot-platform-matrix-report.json",
    "tvm-aot-thread-sanitizer-report.json",
    "vm-multicore-release-closeout.json",
    "aot-compilation-benchmark.json",
    "http-aot-performance-comparison.json",
    "tvm-managed-list-profile.json",
)


def is_sha256(value: object) -> bool:
    """Return whether a value is one canonical lowercase SHA-256 digest."""

    return (
        isinstance(value, str)
        and len(value) == 64
        and all(character in "0123456789abcdef" for character in value)
    )


def run(command: list[str]) -> None:
    """Run one matrix command from the repository root."""

    subprocess.run(command, cwd=ROOT, check=True)


def validate_workflow_text(name: str, workflow: str) -> None:
    """Validate one native matrix workflow without requiring a YAML package."""

    if "  workflow_dispatch:\n" not in workflow:
        raise AssertionError(f"{name} workflow cannot be dispatched for an exact revision")
    rows = workflow.count("          - target:")
    if rows != len(RUNNERS):
        raise AssertionError(f"{name} workflow defines {rows} native matrix rows")
    for target, runner in RUNNERS.items():
        row = f"          - target: {target}\n            runner: {runner}\n"
        if workflow.count(row) != 1:
            raise AssertionError(
                f"{name} workflow omits canonical runner `{runner}` for `{target}`"
            )
    required = (
        "python -B tools/check_tvm_aot_platform_matrix.py target",
        "TERLAN_MATRIX_TARGET: ${{ matrix.target }}",
        "python -B tools/check_tvm_aot_platform_matrix.py aggregate",
        '$installedBin = Join-Path $installed "bin"',
        "Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append",
        "cargo test --locked --no-default-features --features native-codegen -p terlan --bin terlan-vm",
        "retention-days: 90",
        "uses: actions/checkout@v4",
        "uses: actions/upload-artifact@v4",
        "uses: actions/download-artifact@v4",
    )
    for fragment in required:
        if fragment not in workflow:
            raise AssertionError(f"{name} workflow omits `{fragment}`")


def validate_ci_trigger_text(workflow: str) -> None:
    """Require pull-request, main, and dedicated AOT branch CI execution."""

    if "  pull_request:\n" not in workflow:
        raise AssertionError("CI workflow lost pull-request execution")
    branch_contract = '    branches:\n      - main\n      - "agent/aot-*"\n'
    if branch_contract not in workflow:
        raise AssertionError("CI workflow lost main or dedicated AOT branch execution")


def validate_workflow_contract(root: Path = ROOT) -> None:
    """Require CI and release to launch and retain the same six native targets."""

    ci = (root / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    release = (root / ".github/workflows/release.yml").read_text(encoding="utf-8")
    validate_workflow_text("CI", ci)
    validate_workflow_text("release", release)
    validate_ci_trigger_text(ci)
    for path_filter in ('      - "docs/release/evidence/**"', '      - "docs/roadmap/**"'):
        if ci.count(path_filter) != 2:
            raise AssertionError(f"CI workflow does not validate changes matching {path_filter}")
    makefile = (root / "Makefile").read_text(encoding="utf-8")
    if "tvm-aot-roadmap-reconciliation-check:" not in makefile:
        raise AssertionError("repository omits the final AOT roadmap reconciliation gate")
    check_gates = makefile.split("CHECK_GATES :=", 1)[1].split("\n\ncheck:", 1)[0]
    if "tvm-aot-roadmap-reconciliation-check" in check_gates:
        raise AssertionError(
            "ordinary checks cannot require final AOT roadmap reconciliation"
        )
    if '      - "v*"\n' not in release:
        raise AssertionError("release workflow lost version-tag execution")
    if "run: make tvm-aot-release-closeout-check" not in release:
        raise AssertionError("release workflow omits canonical AOT closeout")
    for evidence in RELEASE_EVIDENCE:
        if f"target/quality/{evidence}" not in release:
            raise AssertionError(f"release workflow does not retain `{evidence}`")


def normalized_host() -> tuple[str, str]:
    """Return the host using the release matrix naming contract."""

    system = platform.system().lower()
    os_name = {"darwin": "macos"}.get(system, system)
    machine = platform.machine().lower()
    arch = {"amd64": "x86_64", "arm64": "aarch64"}.get(machine, machine)
    return os_name, arch


def host_target_id() -> str:
    """Return and validate the current native runner identity."""

    target_id = "-".join(normalized_host())
    if target_id not in TARGETS:
        raise AssertionError(f"unsupported TVM AOT matrix host `{target_id}`")
    expected = os.environ.get("TERLAN_MATRIX_TARGET")
    if expected and expected != target_id:
        raise AssertionError(
            f"matrix runner expected `{expected}` but reported `{target_id}`"
        )
    return target_id


def execution_provenance(source_revision: str) -> dict[str, object]:
    """Return local provenance or one complete GitHub Actions run identity."""

    if os.environ.get("GITHUB_ACTIONS", "").lower() != "true":
        return {"execution_environment": "local"}

    names = (
        "GITHUB_REPOSITORY",
        "GITHUB_WORKFLOW_REF",
        "GITHUB_RUN_ID",
        "GITHUB_RUN_ATTEMPT",
        "GITHUB_SHA",
    )
    values = {name: os.environ.get(name, "").strip() for name in names}
    missing = [name for name, value in values.items() if not value]
    if missing:
        raise AssertionError(f"GitHub Actions provenance is incomplete: {missing}")
    if values["GITHUB_REPOSITORY"] != OFFICIAL_REPOSITORY:
        raise AssertionError(
            "platform release evidence must execute in the official repository"
        )
    if not values["GITHUB_SHA"].startswith(source_revision):
        raise AssertionError(
            "checked-out source revision does not match the GitHub Actions commit"
        )
    if not values["GITHUB_RUN_ID"].isdigit() or not values["GITHUB_RUN_ATTEMPT"].isdigit():
        raise AssertionError("GitHub Actions run identity must be numeric")
    return {
        "execution_environment": "github-actions",
        "repository": values["GITHUB_REPOSITORY"],
        "workflow_ref": values["GITHUB_WORKFLOW_REF"],
        "run_id": int(values["GITHUB_RUN_ID"]),
        "run_attempt": int(values["GITHUB_RUN_ATTEMPT"]),
        "commit_sha": values["GITHUB_SHA"],
    }


def cargo_test(binary: str, test: str) -> None:
    """Run one exact platform lifecycle test."""

    run(
        [
            "cargo",
            "test",
            "--locked",
            "--no-default-features",
            "--features",
            "native-codegen",
            "-p",
            "terlan",
            "--bin",
            binary,
            test,
            "--",
            "--exact",
        ]
    )


def build_and_validate_host() -> dict[str, object]:
    """Execute the complete target-native package and lifecycle contract."""

    target_id = host_target_id()
    expected = TARGETS[target_id]
    run(
        [
            "cargo",
            "build",
            "--locked",
            "--no-default-features",
            "--features",
            "native-codegen",
            "-p",
            "terlan",
            "--bin",
            "terlc",
            "--bin",
            "terlan-vm",
        ]
    )
    run([sys.executable, "-B", "tools/check_tvm_package_install_consumer.py"])
    cargo_test(
        "terlc",
        "commands::vm::vm_test::vm_native_reload_executes_two_compiled_generations",
    )
    cargo_test(
        "terlc",
        "commands::vm::vm_test::vm_native_reload_quarantines_timed_out_generation_without_force_unload",
    )
    cargo_test(
        "terlan-vm",
        "runtime::vm::pure_native::execution_shard::execution_shard_test::active_shard_crash_recovery_rejects_early_restart_and_stale_execution",
    )
    run(
        [
            "cargo",
            "build",
            "--locked",
            "--no-default-features",
            "--release",
            "-p",
            "terlan",
            "--features",
            "native-codegen,editor-lsp",
            "--bin",
            "terlc",
            "--bin",
            "terlan-vm",
            "--bin",
            "terlan-native-worker",
            "--bin",
            "terlan-lsp",
        ]
    )
    for command in ("package", "smoke", "installer-smoke"):
        run([sys.executable, "-B", "tools/package_release_artifact.py", command])

    release = json.loads((ROOT / "dist/terlan-release.json").read_text(encoding="utf-8"))
    native = release.get("native_self_test")
    if not isinstance(native, dict):
        raise AssertionError("release omitted native self-test execution metadata")
    for field, value in expected.items():
        actual = release.get(field) if field in ("os", "arch", "target_triple") else native.get(field)
        if actual != value:
            raise AssertionError(
                f"platform `{target_id}` expected {field} `{value}`, found `{actual}`"
            )
    if int(native.get("native_debug_record_count", 0)) < 2:
        raise AssertionError("platform image omitted native debug/stack records")
    if not native.get("continuation_ids"):
        raise AssertionError("platform image omitted continuation identities")

    source_revision = str(release.get("source_revision"))
    return {
        "schema": TARGET_SCHEMA,
        "decision": "pass",
        "target_id": target_id,
        **expected,
        "version": release.get("version"),
        "source_revision": source_revision,
        **execution_provenance(source_revision),
        "descriptor_digest": native.get("descriptor_digest"),
        "image_sha256": native.get("sha256"),
        "continuation_ids": native.get("continuation_ids"),
        "native_debug_record_count": native.get("native_debug_record_count"),
        "executed_checks": list(REQUIRED_EXECUTED_CHECKS),
    }


def write_target_report() -> Path:
    """Run one host target and write its portable attestation."""

    report = build_and_validate_host()
    TARGET_REPORT_DIR.mkdir(parents=True, exist_ok=True)
    path = TARGET_REPORT_DIR / f"{report['target_id']}.json"
    path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"TVM AOT platform target passed: {report['target_id']}")
    return path


def load_target_reports(root: Path) -> dict[str, dict[str, object]]:
    """Load exactly one attestation for every supported target."""

    reports: dict[str, dict[str, object]] = {}
    for path in sorted(root.rglob("*.json")):
        report = json.loads(path.read_text(encoding="utf-8"))
        if report.get("schema") != TARGET_SCHEMA:
            continue
        target_id = str(report.get("target_id", ""))
        if target_id in reports:
            raise AssertionError(f"duplicate platform attestation `{target_id}`")
        reports[target_id] = report
    missing = sorted(set(TARGETS) - set(reports))
    extra = sorted(set(reports) - set(TARGETS))
    if missing or extra:
        raise AssertionError(f"platform attestation mismatch: missing={missing}, extra={extra}")
    return reports


def build_matrix(reports: dict[str, dict[str, object]]) -> dict[str, object]:
    """Validate target reports and return one canonical aggregate record."""

    missing = sorted(set(TARGETS) - set(reports))
    extra = sorted(set(reports) - set(TARGETS))
    if missing or extra:
        raise AssertionError(f"platform attestation mismatch: missing={missing}, extra={extra}")
    revisions = {str(report.get("source_revision")) for report in reports.values()}
    versions = {str(report.get("version")) for report in reports.values()}
    if len(revisions) != 1 or len(versions) != 1:
        raise AssertionError("platform attestations do not describe one source revision and version")
    provenance_fields = (
        "repository",
        "workflow_ref",
        "run_id",
        "run_attempt",
        "commit_sha",
    )
    provenance = {
        field: {report.get(field) for report in reports.values()}
        for field in provenance_fields
    }
    if any(
        report.get("execution_environment") != "github-actions"
        for report in reports.values()
    ):
        raise AssertionError("platform aggregate requires GitHub Actions execution evidence")
    if any(len(values) != 1 or None in values or "" in values for values in provenance.values()):
        raise AssertionError("platform attestations do not describe one CI workflow run")
    if provenance["repository"] != {OFFICIAL_REPOSITORY}:
        raise AssertionError("platform attestations do not belong to the official repository")
    source_revision = revisions.pop()
    commit_sha = str(next(iter(provenance["commit_sha"])))
    if not commit_sha.startswith(source_revision):
        raise AssertionError("platform source revision does not match its CI commit")
    for target_id, expected in TARGETS.items():
        report = reports[target_id]
        if report.get("decision") != "pass":
            raise AssertionError(f"platform `{target_id}` did not pass executable validation")
        for field, value in expected.items():
            if report.get(field) != value:
                raise AssertionError(f"platform `{target_id}` has stale `{field}` metadata")
        if report.get("executed_checks") != list(REQUIRED_EXECUTED_CHECKS):
            raise AssertionError(
                f"platform `{target_id}` has incomplete or noncanonical executable checks"
            )
        for field in ("descriptor_digest", "image_sha256"):
            if not is_sha256(report.get(field)):
                raise AssertionError(
                    f"platform `{target_id}` has invalid artifact `{field}`"
                )
        if not report.get("continuation_ids"):
            raise AssertionError(f"platform `{target_id}` omitted continuation identities")
        debug_count = report.get("native_debug_record_count")
        if not isinstance(debug_count, int) or isinstance(debug_count, bool) or debug_count < 2:
            raise AssertionError(
                f"platform `{target_id}` omitted native debug/stack records"
            )

    return {
        "schema": MATRIX_SCHEMA,
        "decision": "pass",
        "version": versions.pop(),
        "source_revision": source_revision,
        "execution_environment": "github-actions",
        "repository": OFFICIAL_REPOSITORY,
        "workflow_ref": next(iter(provenance["workflow_ref"])),
        "run_id": next(iter(provenance["run_id"])),
        "run_attempt": next(iter(provenance["run_attempt"])),
        "commit_sha": commit_sha,
        "target_count": len(reports),
        "targets": [reports[target_id] for target_id in TARGETS],
        "static_or_skipped_rows": 0,
    }


def aggregate(report_root: Path) -> Path:
    """Require six execution attestations from one official CI workflow run."""

    reports = load_target_reports(report_root)
    matrix = build_matrix(reports)
    MATRIX_REPORT.parent.mkdir(parents=True, exist_ok=True)
    MATRIX_REPORT.write_text(json.dumps(matrix, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"TVM AOT platform matrix passed: {len(reports)} executable targets")
    return MATRIX_REPORT


def self_test() -> None:
    """Prove incomplete, duplicate, skipped, stale, and mixed rows fail closed."""

    validate_workflow_contract()
    ci_text = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
    for invalid in (
        ci_text.replace("  workflow_dispatch:\n", "", 1),
        ci_text.replace("runner: windows-11-arm", "runner: windows-2025", 1),
        ci_text.replace("          - target: linux-x86_64\n", "", 1),
        ci_text.replace(
            "          $installedBin = Join-Path $installed \"bin\"\n", "", 1
        ),
    ):
        try:
            validate_workflow_text("CI fixture", invalid)
        except AssertionError:
            pass
        else:
            raise AssertionError("platform contract accepted an invalid CI workflow")
    for invalid in (
        ci_text.replace("  pull_request:\n", "", 1),
        ci_text.replace('      - "agent/aot-*"\n', "", 1),
    ):
        try:
            validate_ci_trigger_text(invalid)
        except AssertionError:
            pass
        else:
            raise AssertionError("platform contract accepted invalid CI triggers")

    with patch.dict(os.environ, {"GITHUB_ACTIONS": "false"}):
        assert execution_provenance("local-revision") == {
            "execution_environment": "local"
        }
    github_environment = {
        "GITHUB_ACTIONS": "true",
        "GITHUB_REPOSITORY": OFFICIAL_REPOSITORY,
        "GITHUB_WORKFLOW_REF": (
            f"{OFFICIAL_REPOSITORY}/.github/workflows/ci.yml@refs/heads/main"
        ),
        "GITHUB_RUN_ID": "100",
        "GITHUB_RUN_ATTEMPT": "1",
        "GITHUB_SHA": "self-test-revision-full-sha",
    }
    with patch.dict(os.environ, github_environment):
        assert execution_provenance("self-test-revision")["run_id"] == 100
    for field, value in (
        ("GITHUB_RUN_ID", "not-numeric"),
        ("GITHUB_REPOSITORY", "fork/terlan"),
        ("GITHUB_SHA", "unrelated-commit"),
    ):
        invalid_environment = dict(github_environment)
        invalid_environment[field] = value
        with patch.dict(os.environ, invalid_environment):
            try:
                execution_provenance("self-test-revision")
            except AssertionError:
                pass
            else:
                raise AssertionError(
                    f"execution provenance accepted invalid `{field}`"
                )

    with tempfile.TemporaryDirectory(prefix="terlan-tvm-platform-matrix.") as tmp:
        root = Path(tmp)
        valid: dict[str, dict[str, object]] = {}
        for target_id, expected in TARGETS.items():
            report = {
                "schema": TARGET_SCHEMA,
                "decision": "pass",
                "target_id": target_id,
                **expected,
                "version": "self-test",
                "source_revision": "self-test-revision",
                "execution_environment": "github-actions",
                "repository": OFFICIAL_REPOSITORY,
                "workflow_ref": f"{OFFICIAL_REPOSITORY}/.github/workflows/ci.yml@refs/heads/main",
                "run_id": 100,
                "run_attempt": 1,
                "commit_sha": "self-test-revision-full-sha",
                "descriptor_digest": "ab" * 32,
                "image_sha256": "cd" * 32,
                "continuation_ids": [1],
                "native_debug_record_count": 2,
                "executed_checks": list(REQUIRED_EXECUTED_CHECKS),
            }
            valid[target_id] = report
            (root / f"{target_id}.json").write_text(
                json.dumps(report), encoding="utf-8"
            )
        assert build_matrix(load_target_reports(root))["target_count"] == 6

        missing = root / "linux-x86_64.json"
        missing.unlink()
        try:
            load_target_reports(root)
        except AssertionError:
            pass
        else:
            raise AssertionError("incomplete platform matrix passed")
        missing.write_text(json.dumps(valid["linux-x86_64"]), encoding="utf-8")

        duplicate = root / "duplicate" / "linux-x86_64.json"
        duplicate.parent.mkdir()
        duplicate.write_text(json.dumps(valid["linux-x86_64"]), encoding="utf-8")
        try:
            load_target_reports(root)
        except AssertionError:
            pass
        else:
            raise AssertionError("duplicate platform matrix row passed")
        duplicate.unlink()

        for field, value in [
            ("decision", "skipped"),
            ("calling_convention", "forged"),
            ("source_revision", "mixed-revision"),
            ("execution_environment", "local"),
            ("run_id", 101),
            ("commit_sha", "unrelated-commit"),
            ("descriptor_digest", "not-a-digest"),
            ("image_sha256", "CD" * 32),
            ("continuation_ids", []),
            ("native_debug_record_count", 1),
        ]:
            candidate = {target: dict(report) for target, report in valid.items()}
            candidate["windows-aarch64"][field] = value
            try:
                build_matrix(candidate)
            except AssertionError:
                pass
            else:
                raise AssertionError(f"platform matrix accepted invalid `{field}`")

        candidate = {target: dict(report) for target, report in valid.items()}
        candidate["windows-aarch64"]["executed_checks"] = list(
            REQUIRED_EXECUTED_CHECKS[:-1]
        )
        try:
            build_matrix(candidate)
        except AssertionError:
            pass
        else:
            raise AssertionError("platform matrix accepted incomplete executable checks")
    print("TVM AOT platform matrix self-test passed")


def parse_args() -> argparse.Namespace:
    """Parse target and aggregate modes."""

    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    subparsers.add_parser("target")
    subparsers.add_parser("self-test")
    aggregate_parser = subparsers.add_parser("aggregate")
    aggregate_parser.add_argument("report_root", type=Path)
    return parser.parse_args()


def main() -> int:
    """Run one stable matrix operation."""

    args = parse_args()
    try:
        if args.command == "target":
            write_target_report()
        elif args.command == "self-test":
            self_test()
        else:
            aggregate(args.report_root)
    except (OSError, subprocess.CalledProcessError, AssertionError, ValueError) as error:
        print(f"TVM AOT platform matrix check failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
