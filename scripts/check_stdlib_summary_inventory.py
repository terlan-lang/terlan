#!/usr/bin/env python3
"""Validate that every std source module has committed summary artifacts.

Inputs:
- `std/**/*.tl` source files.
- `std/summaries/*.typi` and `.typi.deps` files.

Outputs:
- Exit code `0` when the source/summary inventory is complete.
- Exit code `1` with diagnostics when summaries are missing or stale.

Transformation:
- Maps each source module declaration to its expected generated summary files
  and rejects orphan generated summaries with no source module.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
STD_ROOT = REPO_ROOT / "std"
SUMMARY_DIR = STD_ROOT / "summaries"
MODULE_RE = re.compile(r"(?m)^\s*module\s+([A-Za-z_][A-Za-z0-9_.]*)\s*\.")


def module_name_for_source(source: Path) -> str:
    """Extract the declared module name from one std source file.

    Inputs:
    - `source`: path to a `.tl` file.

    Outputs:
    - Declared module name.

    Transformation:
    - Reads source text and returns the first canonical `module Name.`
      declaration.
    """
    text = source.read_text(encoding="utf-8")
    match = MODULE_RE.search(text)
    if not match:
        raise ValueError(f"{source}: missing module declaration")
    return match.group(1)


def expected_module_names() -> set[str]:
    """Return std module names that require direct summaries.

    Inputs:
    - Repository `std` source tree.

    Outputs:
    - Set of declared std module names.

    Transformation:
    - Parses module declarations from every std `.tl` source file.
    """
    return {module_name_for_source(path) for path in STD_ROOT.glob("**/*.tl")}


def package_summary_names(module_names: set[str]) -> set[str]:
    """Return package summary names implied by direct module summaries.

    Inputs:
    - Direct std module names.

    Outputs:
    - Parent package names with direct child modules.

    Transformation:
    - Drops the final module segment from each multi-segment module name.
    """
    return {".".join(name.split(".")[:-1]) for name in module_names if name.count(".") >= 2}


def main() -> int:
    """Run std summary inventory validation.

    Inputs:
    - Command-line invocation with no arguments.

    Outputs:
    - Process exit code `0` when inventory is correct, else `1`.

    Transformation:
    - Compares expected direct and package summaries against checked-in files.
    """
    try:
        modules = expected_module_names()
    except ValueError as err:
        print(err, file=sys.stderr)
        return 1

    expected_typi = {f"{name}.typi" for name in modules | package_summary_names(modules)}
    expected_deps = {f"{name}.typi.deps" for name in modules}
    actual_typi = {path.name for path in SUMMARY_DIR.glob("*.typi")}
    actual_deps = {path.name for path in SUMMARY_DIR.glob("*.typi.deps")}

    errors: list[str] = []
    for name in sorted(expected_typi - actual_typi):
        errors.append(f"missing summary: std/summaries/{name}")
    for name in sorted(expected_deps - actual_deps):
        errors.append(f"missing dependency manifest: std/summaries/{name}")
    for name in sorted(actual_typi - expected_typi):
        errors.append(f"orphan summary without std source/package: std/summaries/{name}")
    for name in sorted(actual_deps - expected_deps):
        errors.append(f"orphan dependency manifest without std source: std/summaries/{name}")

    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
