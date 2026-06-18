#!/usr/bin/env python3
"""Check generated std `.typi` summaries against committed artifacts.

Inputs:
- Standard-library `.terl` sources under `std/`.
- Committed `.typi` and `.typi.deps` files under `std/summaries/`.

Outputs:
- Exit status 0 when regenerated summaries match committed summaries.
- Exit status 1 with path-specific diagnostics when generation fails, a
  committed summary is stale, or an expected committed summary is missing.

Transformation:
- Runs `std/scripts/build_interfaces.py` into a temporary directory.
- Compares generated `.typi` and `.typi.deps` files byte-for-byte against the
  committed release artifacts.
"""

from __future__ import annotations

import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SUMMARIES = ROOT / "std" / "summaries"
BUILD_INTERFACES = ROOT / "std" / "scripts" / "build_interfaces.py"
SUMMARY_SUFFIXES = (".typi", ".typi.deps")
PACKAGE_SUMMARIES = {"std.core.typi", "std.http.typi", "std.io.typi"}


@dataclass(frozen=True)
class DriftDiagnostic:
    """Summary drift diagnostic.

    Inputs:
    - `path`: repository-relative path for the affected summary.
    - `message`: human-readable drift reason.

    Outputs:
    - Immutable diagnostic printed by the drift checker.

    Transformation:
    - Pairs the affected artifact with a stable reason so release logs are
      easy to scan.
    """

    path: Path
    message: str

    def render(self) -> str:
        """Return this diagnostic as display text.

        Inputs:
        - Diagnostic path and message.

        Outputs:
        - One-line diagnostic string.

        Transformation:
        - Formats repository-relative paths consistently for shell output.
        """

        return f"{self.path}: {self.message}"


def is_summary_artifact(path: Path) -> bool:
    """Return whether a path is a checked summary artifact.

    Inputs:
    - `path`: artifact path to classify.

    Outputs:
    - `True` for `.typi` and `.typi.deps` files.
    - `False` for all other artifacts.

    Transformation:
    - Applies the 0.0.4 generated-artifact policy for committed std summaries.
    """

    return any(path.name.endswith(suffix) for suffix in SUMMARY_SUFFIXES)


def run_generation(out_dir: Path) -> str | None:
    """Regenerate std summaries into a temporary directory.

    Inputs:
    - `out_dir`: temporary output directory.

    Outputs:
    - `None` when generation succeeds.
    - Combined stdout/stderr text when generation fails.

    Transformation:
    - Calls the normal std interface generator with a temp output override so
      drift validation never mutates committed summary files.
    """

    result = subprocess.run(
        [sys.executable, str(BUILD_INTERFACES), "--out-dir", str(out_dir)],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        return (result.stdout + result.stderr).rstrip()
    return None


def compare_generated(out_dir: Path) -> list[DriftDiagnostic]:
    """Compare generated summary artifacts to committed files.

    Inputs:
    - `out_dir`: directory containing regenerated summary artifacts.

    Outputs:
    - Diagnostics for missing or stale committed summary files.

    Transformation:
    - Walks generated `.typi` and `.typi.deps` artifacts, locates the matching
      committed file, and compares bytes.
    """

    diagnostics: list[DriftDiagnostic] = []
    generated = sorted(path for path in out_dir.iterdir() if path.is_file() and is_summary_artifact(path))
    for artifact in generated:
        committed = SUMMARIES / artifact.name
        relative = committed.relative_to(ROOT)
        if not committed.is_file():
            diagnostics.append(DriftDiagnostic(relative, "missing committed generated summary"))
            continue
        if artifact.read_bytes() != committed.read_bytes():
            diagnostics.append(DriftDiagnostic(relative, "committed summary is stale; run make stdlib-build-interfaces"))
    return diagnostics


def check_stale_committed(generated_dir: Path) -> list[DriftDiagnostic]:
    """Return committed summary files not produced by regeneration.

    Inputs:
    - `generated_dir`: directory containing regenerated summary artifacts.

    Outputs:
    - Diagnostics for committed `.typi` or `.typi.deps` files that no source
      module generated.

    Transformation:
    - Compares committed summary filenames with generated filenames.
    """

    generated_names = {
        path.name for path in generated_dir.iterdir() if path.is_file() and is_summary_artifact(path)
    }
    diagnostics: list[DriftDiagnostic] = []
    for committed in sorted(path for path in SUMMARIES.iterdir() if path.is_file() and is_summary_artifact(path)):
        if committed.name in PACKAGE_SUMMARIES:
            continue
        if committed.name not in generated_names:
            diagnostics.append(
                DriftDiagnostic(
                    committed.relative_to(ROOT),
                    "committed summary was not regenerated from std source",
                )
            )
    return diagnostics


def main() -> int:
    """Run std summary drift validation.

    Inputs:
    - Standard-library source tree and committed summary artifacts.

    Outputs:
    - Exit status 0 when committed summaries are current.
    - Exit status 1 when generation fails or drift is detected.

    Transformation:
    - Regenerates summaries into a temporary directory and compares generated
      artifacts against committed release-owned summary files.
    """

    with tempfile.TemporaryDirectory(prefix="terlan-std-summary-drift-") as temp:
        out_dir = Path(temp)
        failure = run_generation(out_dir)
        if failure is not None:
            print("[stdlib-summary-drift] generation failed:", file=sys.stderr)
            print(failure, file=sys.stderr)
            return 1

        diagnostics = compare_generated(out_dir)
        diagnostics.extend(check_stale_committed(out_dir))
        if diagnostics:
            print("[stdlib-summary-drift] failures:", file=sys.stderr)
            for diagnostic in diagnostics:
                print(f"  - {diagnostic.render()}", file=sys.stderr)
            return 1

        generated_count = len([path for path in out_dir.iterdir() if path.is_file() and is_summary_artifact(path)])
        print(f"[stdlib-summary-drift] {generated_count} generated summaries match committed artifacts.")
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
