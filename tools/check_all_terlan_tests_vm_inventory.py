#!/usr/bin/env python3
"""Inventory Terlan tests that must run on the VM lane.

Inputs:
- Standard-library `*Test.terl` files.
- Repository `tests/**/*.terl` source fixtures.
- Active `terlc test`, `terlc run`, and `terlc repl` implementation sources.
- Active Make/script runner files that execute test lanes.
- Active Terlan project manifests outside historical release-candidate
  fixtures.

Outputs:
- Exit status 0 with inventory counts when the active test lane is VM-owned.
- Exit status 1 with location diagnostics when active tests or runners depend
  on `erl`, `erlc`, EUnit, BEAM artifacts, or `beam-thin`.

Transformation:
- Separates active VM test surfaces from historical/negative compatibility
  fixtures so removed OTP spellings can remain rejection coverage while new
  executable test paths cannot drift back to OTP.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
STD_ROOT = ROOT / "std"
TESTS_ROOT = ROOT / "tests"
COMMAND_ROOTS = (
    ROOT / "crates" / "terlan" / "src" / "commands" / "test",
    ROOT / "crates" / "terlan" / "src" / "commands" / "run",
    ROOT / "crates" / "terlan" / "src" / "commands" / "repl",
)
RUNNER_FILES = (
    ROOT / "Makefile",
    ROOT / "std" / "stdlib.mk",
    ROOT / "std" / "scripts" / "run_release_tests.sh",
    ROOT / "scripts" / "run_exact_cargo_test.sh",
)
HISTORICAL_MANIFEST_ROOTS = (
    ROOT / "tests" / "rc",
)
FORBIDDEN_TEXT_PATTERNS = (
    (re.compile(r"\berlc\b"), "uses `erlc`"),
    (re.compile(r"\bCommand::new\(\s*[\"']erl[\"']\s*\)"), "launches `erl`"),
    (re.compile(r"\bCommand::new\(\s*[\"']erlc[\"']\s*\)"), "launches `erlc`"),
    (re.compile(r"\beunit\b", re.IGNORECASE), "depends on EUnit"),
    (re.compile(r"_build/ebin"), "depends on BEAM ebin output"),
)
ALLOWED_NEGATIVE_CONTEXTS = (
    "without invoking generated Erlang, BEAM bytecode, or EUnit",
    "test ! -d /tmp/terlan_vm_execution_check/_build/ebin",
)
FORBIDDEN_MANIFEST_PATTERNS = (
    (re.compile(r'artifact\s*=\s*"beam-thin"'), 'declares `artifact = "beam-thin"`'),
    (re.compile(r'target\s*=\s*"erlang"'), 'declares `target = "erlang"`'),
    (re.compile(r'runtime\s*=\s*"beam"'), 'declares `runtime = "beam"`'),
)
MIN_STD_TESTS = 1
MIN_REPOSITORY_TESTS = 1


@dataclass(frozen=True)
class Finding:
    """VM test inventory finding.

    Inputs:
    - `path`: source file that owns the violation.
    - `line`: optional one-based line number.
    - `message`: stable human-readable diagnostic.

    Outputs:
    - Immutable finding used by gate output.

    Transformation:
    - Keeps source location and reason together so the gate remains actionable.
    """

    path: Path
    line: int | None
    message: str

    def render(self) -> str:
        """Render the finding as a stable repository-relative diagnostic."""

        relative = self.path.relative_to(ROOT)
        if self.line is None:
            return f"{relative}: {self.message}"
        return f"{relative}:{self.line}: {self.message}"


def source_files(root: Path, pattern: str) -> list[Path]:
    """Return sorted source files under a root.

    Inputs:
    - `root`: directory to scan.
    - `pattern`: glob accepted by `Path.rglob`.

    Outputs:
    - Sorted file paths, or an empty list when the root is absent.

    Transformation:
    - Normalizes discovery for optional roots so local checkouts without a
      particular fixture directory fail through explicit minimum-count checks
      rather than filesystem errors.
    """

    if not root.exists():
        return []
    return sorted(path for path in root.rglob(pattern) if path.is_file())


def implementation_files() -> list[Path]:
    """Return active command implementation files for VM test closure.

    Inputs:
    - `COMMAND_ROOTS`.

    Outputs:
    - Sorted Rust and README files excluding Rust test modules.

    Transformation:
    - Keeps rejection tests out of forbidden-token scanning because they
      intentionally mention removed OTP spellings, while implementation files
      must not invoke those paths.
    """

    files: list[Path] = []
    for root in COMMAND_ROOTS:
        if not root.exists():
            continue
        for path in sorted(root.rglob("*")):
            if not path.is_file():
                continue
            if path.name.endswith("_test.rs"):
                continue
            if path.suffix == ".rs" or path.name == "README.md":
                files.append(path)
    return files


def active_manifests() -> list[Path]:
    """Return active `terlan.toml` manifests checked by the VM test gate.

    Inputs:
    - Repository tree.

    Outputs:
    - Sorted manifests excluding target directories and historical release
      candidate fixtures.

    Transformation:
    - Treats old release-candidate manifests as provenance data, not active VM
      test lanes.
    """

    manifests: list[Path] = []
    for path in sorted(ROOT.rglob("terlan.toml")):
        if any(part in {"target", "node_modules"} for part in path.parts):
            continue
        if any(path.is_relative_to(root) for root in HISTORICAL_MANIFEST_ROOTS):
            continue
        manifests.append(path)
    return manifests


def active_runner_files() -> list[Path]:
    """Return active test-runner files checked for VM-only execution.

    Inputs:
    - `RUNNER_FILES`.

    Outputs:
    - Existing runner paths.

    Transformation:
    - Limits scanning to files that actually orchestrate release/test lanes so
      diagnostic tools can still contain forbidden-token patterns they enforce.
    """

    return sorted(path for path in RUNNER_FILES if path.exists())


def scan_text_files(paths: list[Path]) -> list[Finding]:
    """Return forbidden OTP/BEAM findings in source text files."""

    findings: list[Finding] = []
    for path in paths:
        text = path.read_text(encoding="utf-8")
        for line_number, line in enumerate(text.splitlines(), start=1):
            if any(fragment in line for fragment in ALLOWED_NEGATIVE_CONTEXTS):
                continue
            for pattern, message in FORBIDDEN_TEXT_PATTERNS:
                if pattern.search(line):
                    findings.append(Finding(path=path, line=line_number, message=message))
    return findings


def scan_manifests(paths: list[Path]) -> list[Finding]:
    """Return forbidden runtime artifact findings in active manifests."""

    findings: list[Finding] = []
    for path in paths:
        text = path.read_text(encoding="utf-8")
        for line_number, line in enumerate(text.splitlines(), start=1):
            for pattern, message in FORBIDDEN_MANIFEST_PATTERNS:
                if pattern.search(line):
                    findings.append(Finding(path=path, line=line_number, message=message))
    return findings


def minimum_count_findings(std_tests: list[Path], repository_tests: list[Path]) -> list[Finding]:
    """Return findings when required VM test inventories are empty."""

    findings: list[Finding] = []
    if len(std_tests) < MIN_STD_TESTS:
        findings.append(
            Finding(
                path=STD_ROOT,
                line=None,
                message="expected at least one standard-library VM test fixture",
            )
        )
    if len(repository_tests) < MIN_REPOSITORY_TESTS:
        findings.append(
            Finding(
                path=TESTS_ROOT,
                line=None,
                message="expected at least one repository VM test fixture",
            )
        )
    return findings


def main() -> int:
    """Run the VM test closure inventory gate."""

    std_tests = source_files(STD_ROOT, "*Test.terl")
    repository_tests = source_files(TESTS_ROOT, "*.terl")
    implementations = implementation_files()
    runners = active_runner_files()
    manifests = active_manifests()
    findings: list[Finding] = []
    findings.extend(minimum_count_findings(std_tests, repository_tests))
    findings.extend(scan_text_files(std_tests + repository_tests + implementations + runners))
    findings.extend(scan_manifests(manifests))

    if findings:
        for finding in findings:
            print(finding.render(), file=sys.stderr)
        return 1

    print("all_terlan_tests_vm_inventory:")
    print(f"  std_test_files={len(std_tests)}")
    print(f"  repository_test_files={len(repository_tests)}")
    print(f"  command_implementation_files={len(implementations)}")
    print(f"  active_runner_files={len(runners)}")
    print(f"  active_manifests={len(manifests)}")
    print("  forbidden_otp_runtime_paths=0")
    return 0


if __name__ == "__main__":
    sys.exit(main())
