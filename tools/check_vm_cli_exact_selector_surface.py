#!/usr/bin/env python3
"""Check VM CLI exact-selector coverage in the release gates.

Inputs:
- The repository `Makefile`.

Outputs:
- Exit status 0 when VM REPL, run, test, and artifact execution exact
  selectors are present in release gates.
- Exit status 1 with stable diagnostics when a required selector or target is
  missing.

Transformation:
- Treats exact-selector coverage as a release contract so CLI VM surfaces
  cannot silently lose focused test coverage while broader checks still pass.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
MAKEFILE = ROOT / "Makefile"
REQUIRED_TARGETS = (
    "terlan-vm-repl-check",
    "terlan-vm-run-command-check",
    "terlan-vm-test-command-check",
)
REQUIRED_SELECTORS = (
    (
        "terlc repl VM execution",
        "commands::repl::repl_test::repl_expression_runs_arithmetic_through_vm_runtime",
    ),
    (
        "terlc run VM default",
        "commands::run::run_test::validate_run_args_defaults_to_vm_target",
    ),
    (
        "terlc run VM artifact execution",
        "commands::run::run_test::run_built_vm_artifact_executes_vm_runner",
    ),
    (
        "terlc test VM default",
        "commands::test::test_command_test::run_test_defaults_to_terlan_vm_execution",
    ),
    (
        "terlc test project VM default",
        "commands::test::test_command_test::run_project_directory_tests_default_to_vm_and_prepare_source_roots",
    ),
)


@dataclass(frozen=True)
class Finding:
    """VM CLI exact-selector gate finding."""

    message: str

    def render(self) -> str:
        """Render the finding with a stable repository-relative path."""

        return f"{MAKEFILE.relative_to(ROOT)}: {self.message}"


def target_pattern(name: str) -> re.Pattern[str]:
    """Return a Make target declaration matcher."""

    return re.compile(rf"^{re.escape(name)}:", re.MULTILINE)


def missing_targets(text: str) -> list[Finding]:
    """Return findings for required Make targets that are absent."""

    return [
        Finding(message=f"missing VM CLI exact-selector target `{target}`")
        for target in REQUIRED_TARGETS
        if not target_pattern(target).search(text)
    ]


def missing_selectors(text: str) -> list[Finding]:
    """Return findings for required exact selectors that are absent."""

    return [
        Finding(message=f"missing VM CLI exact selector for {label}: `{selector}`")
        for label, selector in REQUIRED_SELECTORS
        if selector not in text
    ]


def main() -> int:
    """Run the VM CLI exact-selector surface check."""

    text = MAKEFILE.read_text(encoding="utf-8")
    findings = missing_targets(text)
    findings.extend(missing_selectors(text))
    if findings:
        for finding in findings:
            print(finding.render(), file=sys.stderr)
        return 1
    print("VM CLI exact-selector surface covers repl, run, test, and artifact execution.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
