#!/usr/bin/env python3
"""Check that stdlib release tests use the VM-default test lane.

Inputs:
- `std/scripts/run_release_tests.sh`.

Outputs:
- Exit status 0 when non-JS stdlib release tests run as bare
  `terlc test <file>` once per unique file and generated JS std contract rows
  opt out of execution.
- Exit status 1 with stable diagnostics when the runner adds explicit VM
  target flags, reintroduces Erlang/BEAM execution, or stops preserving the JS
  exception.

Transformation:
- Scans the release runner for required command and contract-row markers.
  This is a contract check for the release script; the release-scale execution
  itself remains owned by `make stdlib-release-tests`.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / "std" / "scripts" / "run_release_tests.sh"
REQUIRED_MARKERS = (
    "`terlc test` on the default VM lane",
    "target_args=()",
    'std/js/*)',
    "target_args=(--target js --target-profile js.browser)",
    'kind = ($1 ~ /^std\\.js\\..*\\.generated_surface$/) ? "contract" : "test"',
    'if [[ "$row_kind" == contract ]]; then',
    "[stdlib-release-contract]",
    "declare -A executed_test_files=()",
    'if [[ -n "${executed_test_files[$test_file]:-}" ]]; then',
    'executed_test_files["$test_file"]=1',
    'timeout "${test_timeout_seconds}s" "$terlc_bin" test "$test_file" "${target_args[@]}"',
)
FORBIDDEN_PATTERNS = (
    (re.compile(r"--target\s+terlan-vm"), "must not pass explicit VM target; use default lane"),
    (re.compile(r"--target\s+erlang"), "must not pass removed Erlang target"),
    (re.compile(r"--runtime\s+beam"), "must not pass removed BEAM runtime"),
    (re.compile(r"\berlc\b"), "must not call `erlc`"),
    (re.compile(r'(^|[^\w])erl(\s|$)'), "must not call `erl`"),
)


@dataclass(frozen=True)
class Finding:
    """Release-test VM-default contract finding."""

    line: int | None
    message: str

    def render(self) -> str:
        """Render a stable diagnostic line."""

        path = RUNNER.relative_to(ROOT)
        if self.line is None:
            return f"{path}: {self.message}"
        return f"{path}:{self.line}: {self.message}"


def missing_marker_findings(text: str) -> list[Finding]:
    """Return findings for required release-runner markers that are absent."""

    return [
        Finding(line=None, message=f"missing stdlib VM-default release-test marker `{marker}`")
        for marker in REQUIRED_MARKERS
        if marker not in text
    ]


def forbidden_pattern_findings(text: str) -> list[Finding]:
    """Return findings for forbidden runtime fragments in the runner."""

    findings: list[Finding] = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        if "Removed runtime spellings remain rejection coverage" in line:
            continue
        for pattern, message in FORBIDDEN_PATTERNS:
            if pattern.search(line):
                findings.append(Finding(line=line_number, message=message))
    return findings


def main() -> int:
    """Run the stdlib release-test VM-default gate."""

    text = RUNNER.read_text(encoding="utf-8")
    findings = missing_marker_findings(text)
    findings.extend(forbidden_pattern_findings(text))
    if findings:
        for finding in findings:
            print(finding.render(), file=sys.stderr)
        return 1
    print("stdlib release tests use bare terlc test for VM-default lane.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
