#!/usr/bin/env python3
"""Adversarial checks for Terlan release packaging.

Inputs:
- `tools/package_release_artifact.py`.

Outputs:
- Exit status 0 when hostile release packaging inputs fail closed.
- Exit status 1 with stable diagnostics when the packager accepts invalid
  platform metadata or hides missing artifacts.

Transformation:
- Runs the release artifact helper as a subprocess with unsupported OS/arch
  overrides and with a missing artifact smoke path. This validates the public
  helper boundary rather than importing implementation internals.
"""

from __future__ import annotations

import os
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
PACKAGE_HELPER = ROOT / "tools" / "package_release_artifact.py"


@dataclass(frozen=True)
class PackagingAdversarialCase:
    """One release-packaging adversarial subprocess case."""

    name: str
    args: list[str]
    env: dict[str, str]
    expected_fragment: str


def run_case(case: PackagingAdversarialCase) -> str | None:
    """Run one adversarial package-helper case.

    Inputs:
    - `case`: command, environment overrides, and expected diagnostic text.

    Outputs:
    - `None` when the helper fails with the expected diagnostic.
    - Diagnostic text when the helper succeeds or reports the wrong failure.

    Transformation:
    - Executes the helper in a subprocess so argument parsing, environment
      normalization, and top-level error rendering are all covered.
    """

    env = os.environ.copy()
    env.update(case.env)
    result = subprocess.run(
        [sys.executable, "-B", str(PACKAGE_HELPER), *case.args],
        cwd=ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    output = result.stdout + result.stderr
    if result.returncode == 0:
        return f"{case.name}: expected failure, command succeeded with output: {output.strip()}"
    if case.expected_fragment not in output:
        return (
            f"{case.name}: expected diagnostic containing "
            f"{case.expected_fragment!r}, got: {output.strip()}"
        )
    return None


def main() -> int:
    """Run release-packaging adversarial checks."""

    cases = [
        PackagingAdversarialCase(
            name="unsupported-os",
            args=["describe"],
            env={"TERLAN_RELEASE_OS": "Solaris", "TERLAN_RELEASE_ARCH": "x86_64"},
            expected_fragment="unsupported release OS `Solaris`",
        ),
        PackagingAdversarialCase(
            name="unsupported-arch",
            args=["describe"],
            env={"TERLAN_RELEASE_OS": "Linux", "TERLAN_RELEASE_ARCH": "riscv64"},
            expected_fragment="unsupported release architecture `riscv64`",
        ),
        PackagingAdversarialCase(
            name="missing-artifact-smoke",
            args=["smoke"],
            env={
                "TERLAN_RELEASE_OS": "Linux",
                "TERLAN_RELEASE_ARCH": "aarch64",
            },
            expected_fragment="release artifact is missing",
        ),
    ]

    failures = [failure for case in cases if (failure := run_case(case)) is not None]
    if failures:
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1
    print("Release packaging adversarial checks passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
