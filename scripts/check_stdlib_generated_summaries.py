#!/usr/bin/env python3
"""Verify committed std summaries match compiler-generated summaries.

Inputs:
- Checked-in `std/**/*.tl` source files.
- Checked-in `std/summaries/*.typi` and `.typi.deps` artifacts.

Outputs:
- Exit code `0` when committed summaries match generation.
- Exit code `1` with a unified diff when any generated summary differs.

Transformation:
- Runs `scripts/build_stdlib_interfaces.py` against a temporary copy of the
  repository, then compares generated artifacts with the current checkout.
"""

from __future__ import annotations

import filecmp
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
SUMMARY_DIR = REPO_ROOT / "std" / "summaries"


def copy_repo_to_temp(target: Path) -> Path:
    """Copy the repository into a temporary work tree.

    Inputs:
    - `target`: empty temporary directory.

    Outputs:
    - Path to the copied repository root.

    Transformation:
    - Copies source files while excluding bulky build and VCS directories so
      summary generation can mutate the copy safely.
    """
    copied = target / "repo"
    ignore = shutil.ignore_patterns("target", ".git", "dist", "_build", ".terlan")
    shutil.copytree(REPO_ROOT, copied, ignore=ignore)
    return copied


def run_generation(repo: Path) -> None:
    """Generate std summaries inside a copied repository.

    Inputs:
    - `repo`: temporary repository copy.

    Outputs:
    - Raises `CalledProcessError` when generation fails.

    Transformation:
    - Invokes the committed generation script so CI checks the same command
      developers use to refresh summaries.
    """
    subprocess.run(
        [sys.executable, "scripts/build_stdlib_interfaces.py"],
        cwd=repo,
        check=True,
    )


def summary_files(root: Path) -> set[str]:
    """Return generated summary artifact names below one root.

    Inputs:
    - `root`: summary directory.

    Outputs:
    - Relative filenames for `.typi` and `.typi.deps` artifacts.

    Transformation:
    - Selects compiler summary artifacts and excludes README-like docs.
    """
    return {path.name for path in root.glob("*.typi")} | {
        path.name for path in root.glob("*.typi.deps")
    }


def diff_text(expected: Path, actual: Path) -> str:
    """Return a unified diff for two summary files.

    Inputs:
    - `expected`: generated file path.
    - `actual`: committed file path.

    Outputs:
    - Unified diff text.

    Transformation:
    - Delegates to `diff -u` so diagnostics match common CI output.
    """
    result = subprocess.run(
        ["diff", "-u", str(actual), str(expected)],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    return result.stdout


def main() -> int:
    """Run generated-summary consistency validation.

    Inputs:
    - Command-line invocation with no arguments.

    Outputs:
    - Process exit code `0` when committed summaries match generated output.

    Transformation:
    - Generates summaries in isolation and compares filenames plus file
      contents against the committed summary directory.
    """
    with tempfile.TemporaryDirectory(prefix="terlan-stdlib-summary-check-") as tmp:
        copied_repo = copy_repo_to_temp(Path(tmp))
        run_generation(copied_repo)
        generated_dir = copied_repo / "std" / "summaries"

        expected = summary_files(generated_dir)
        actual = summary_files(SUMMARY_DIR)
        errors: list[str] = []
        for name in sorted(expected - actual):
            errors.append(f"missing committed generated summary: std/summaries/{name}")
        for name in sorted(actual - expected):
            errors.append(f"stale committed generated summary: std/summaries/{name}")
        for name in sorted(expected & actual):
            generated = generated_dir / name
            committed = SUMMARY_DIR / name
            if not filecmp.cmp(generated, committed, shallow=False):
                errors.append(f"summary differs: std/summaries/{name}")
                errors.append(diff_text(generated, committed))

    if errors:
        print("\n".join(errors), file=sys.stderr)
        print(
            "Run `make stdlib-build-interfaces` to refresh generated summaries.",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
