#!/usr/bin/env python3
"""Verify stdlib source modules have cached `.typi` summaries.

Inputs:
- `std/**/*.terl` source files outside `std/summaries`.
- `std/summaries/*.typi` and matching `.typi.deps` files.

Outputs:
- Exit status 0 when every source module has a matching summary and deps file.
- Exit status 1 with path-specific diagnostics when a module declaration,
  summary file, or deps file is missing.

Transformation:
- Reads module declarations from source files.
- Maps each module name to the deterministic summary filename
  `<module>.typi`.
- Compares the required summary/deps set against files in `std/summaries`.
"""

from __future__ import annotations

import re
import sys
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
STD_DIR = ROOT / "std"
SUMMARY_DIR = STD_DIR / "summaries"
MODULE_RE = re.compile(r"^\s*module\s+([A-Za-z_][A-Za-z0-9_.]*)\s*\.\s*$")


@dataclass(frozen=True)
class SourceModule:
    """Source module discovered from a stdlib `.terl` file.

    Inputs:
    - `path`: absolute path to the source file.
    - `module`: module declaration text from the source file.

    Outputs:
    - Immutable source-module record used by inventory checks.

    Transformation:
    - Holds source path and module name together so diagnostics can point back
      to the file that owns a required summary.
    """

    path: Path
    module: str

    def summary_name(self) -> str:
        """Return the required `.typi` filename for this module.

        Inputs:
        - The `module` field on this source-module record.

        Outputs:
        - Summary filename in `<module>.typi` form.

        Transformation:
        - Appends the `.typi` extension without changing dot or underscore
          module spelling, matching current emitter output.
        """

        return f"{self.module}.typi"

    def deps_name(self) -> str:
        """Return the required `.typi.deps` filename for this module.

        Inputs:
        - The summary filename derived from this source-module record.

        Outputs:
        - Dependency-manifest filename in `<module>.typi.deps` form.

        Transformation:
        - Appends `.deps` to the deterministic summary filename.
        """

        return f"{self.summary_name()}.deps"


def iter_std_sources() -> list[Path]:
    """Return standard-library source files that require summaries.

    Inputs:
    - The repository's `std/` directory.

    Outputs:
    - Sorted list of `.terl` source paths outside `std/summaries`.

    Transformation:
    - Recursively scans `std/`, filters out summary and disabled scratch
      artifacts, and returns a deterministic path order for stable diagnostics.
    """

    return sorted(
        path
        for path in STD_DIR.rglob("*.terl")
        if path.is_file()
        and not is_test_source_name(path.name)
        and "summaries" not in path.relative_to(STD_DIR).parts
        and "disabled" not in path.relative_to(STD_DIR).parts
    )


def is_test_source_name(name: str) -> bool:
    """Return whether a filename is a Terlan test source.

    Inputs:
    - `name`: filesystem basename for a candidate source file.

    Output:
    - `True` when the file uses the canonical `*Test.terl` source suffix.

    Transformation:
    - Keeps std summary inventory aligned with `terlc test` source discovery.
    """

    return name.endswith("Test.terl")


def read_module(path: Path) -> SourceModule | str:
    """Read a module declaration from a Terlan source file.

    Inputs:
    - `path`: source file to inspect.

    Outputs:
    - `SourceModule` when a module declaration is found.
    - Diagnostic string when no module declaration is found.

    Transformation:
    - Scans lines until the first `module ... .` declaration and captures its
      module name without parsing the full source file.
    """

    for line in path.read_text(encoding="utf-8").splitlines():
        match = MODULE_RE.match(line)
        if match:
            return SourceModule(path=path, module=match.group(1))
    return f"{path.relative_to(ROOT)}: missing module declaration"


def collect_modules() -> tuple[list[SourceModule], list[str]]:
    """Collect source modules and module-declaration diagnostics.

    Inputs:
    - Standard-library source paths from `iter_std_sources`.

    Outputs:
    - Pair of discovered `SourceModule` records and diagnostic strings.

    Transformation:
    - Reads each source file once and separates successful module records from
      missing-declaration diagnostics.
    """

    modules: list[SourceModule] = []
    diagnostics: list[str] = []
    for source in iter_std_sources():
        result = read_module(source)
        if isinstance(result, SourceModule):
            modules.append(result)
        else:
            diagnostics.append(result)
    return modules, diagnostics


def check_inventory(modules: list[SourceModule]) -> list[str]:
    """Validate required summary and dependency files exist.

    Inputs:
    - Discovered source-module records.
    - Files currently present in `std/summaries`.

    Outputs:
    - Diagnostic strings for missing summaries or dependency manifests.

    Transformation:
    - Maps each source module to expected summary/deps filenames and checks
      those paths in the summary directory.
    """

    diagnostics: list[str] = []
    for module in modules:
        summary = SUMMARY_DIR / module.summary_name()
        deps = SUMMARY_DIR / module.deps_name()
        source = module.path.relative_to(ROOT)
        if not summary.is_file():
            diagnostics.append(f"{source}: missing summary {summary.relative_to(ROOT)}")
        if not deps.is_file():
            diagnostics.append(f"{source}: missing summary deps {deps.relative_to(ROOT)}")
    return diagnostics


def main() -> int:
    """Run the stdlib summary inventory check.

    Inputs:
    - Process cwd-independent repository paths rooted at this script location.

    Outputs:
    - Exit status 0 on complete inventory.
    - Exit status 1 when source declarations or summary artifacts are missing.

    Transformation:
    - Collects module declarations, validates matching summary artifacts, and
      prints a concise inventory result.
    """

    modules, diagnostics = collect_modules()
    diagnostics.extend(check_inventory(modules))
    if diagnostics:
        print("[stdlib-summary-inventory] failures:", file=sys.stderr)
        for diagnostic in diagnostics:
            print(f"  - {diagnostic}", file=sys.stderr)
        return 1
    print(f"[stdlib-summary-inventory] {len(modules)} stdlib modules have summaries.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
