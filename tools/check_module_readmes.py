#!/usr/bin/env python3
"""Check module-directory README coverage against a migration baseline.

Inputs:
- Directories under `crates/` containing Rust files or crate manifests.
- Directories under `std/` containing Terlan source or interface files.
- `tools/quality/module_readme_missing_baseline.txt`.

Outputs:
- Exit status 0 when all new module directories have README files and baseline
  rows remain current.
- Exit status 1 with path-specific diagnostics otherwise.

Transformation:
- Discovers module directories, compares missing README files against the
  checked-in migration baseline, and fails when new undocumented modules appear.
"""

from __future__ import annotations

from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CRATES = ROOT / "crates"
STD = ROOT / "std"
BASELINE = ROOT / "tools" / "quality" / "module_readme_missing_baseline.txt"


def is_module_directory(path: Path) -> bool:
    """Return whether a directory owns Rust module or crate source.

    Inputs:
    - `path`: directory under `crates/`.

    Outputs:
    - `True` when the directory directly contains `.rs` files or `Cargo.toml`.
    - `False` otherwise.

    Transformation:
    - Checks direct children only so the README requirement applies to the
      directory that owns the files, not every ancestor.
    """

    return any(child.name == "Cargo.toml" or child.suffix == ".rs" for child in path.iterdir())


def is_std_module_directory(path: Path) -> bool:
    """Return whether a directory owns Terlan standard-library source.

    Inputs:
    - `path`: directory under `std/`.

    Outputs:
    - `True` when the directory directly contains `.terl` or `.terli` files.
    - `False` otherwise.

    Transformation:
    - Checks direct children only so generated summary directories and parent
      grouping directories are not treated as module owners unless they contain
      source themselves.
    """

    return any(child.suffix in {".terl", ".terli"} for child in path.iterdir())


def module_directories() -> set[Path]:
    """Return module directories that require README coverage.

    Inputs:
    - Repository `crates/` and `std/` directories.

    Outputs:
    - Set of repository-relative directories with direct Rust or Terlan source
      ownership.

    Transformation:
    - Recursively scans crate directories for Rust ownership and standard
      library directories for Terlan source ownership.
    """

    crate_modules = {
        path.relative_to(ROOT)
        for path in CRATES.rglob("*")
        if path.is_dir() and is_module_directory(path)
    } | ({Path("crates")} if is_module_directory(CRATES) else set())
    std_modules = {
        path.relative_to(ROOT)
        for path in STD.rglob("*")
        if path.is_dir() and is_std_module_directory(path)
    } | ({Path("std")} if is_std_module_directory(STD) else set())
    return crate_modules | std_modules


def read_baseline() -> tuple[set[Path], list[str]]:
    """Read missing-README migration baseline rows.

    Inputs:
    - `tools/quality/module_readme_missing_baseline.txt`.

    Outputs:
    - Set of repository-relative directories allowed to be missing README.md.
    - Diagnostics for malformed rows.

    Transformation:
    - Parses one directory path per line while allowing comments and blanks.
    """

    baseline: set[Path] = set()
    diagnostics: list[str] = []
    for number, line in enumerate(BASELINE.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        if "\t" in line:
            diagnostics.append(f"{BASELINE}:{number}: expected one directory path per line")
            continue
        baseline.add(Path(line))
    return baseline, diagnostics


def check_readmes(modules: set[Path], baseline: set[Path]) -> list[str]:
    """Validate module-directory README coverage.

    Inputs:
    - Current module directories.
    - Missing-README baseline directories.

    Outputs:
    - Diagnostics for new missing READMEs and stale baseline rows.

    Transformation:
    - Allows current documentation debt only while preventing new module
      directories without README.md.
    """

    diagnostics: list[str] = []
    missing = {path for path in modules if not (ROOT / path / "README.md").is_file()}
    for path in sorted(baseline):
        if path not in modules:
            diagnostics.append(f"{path}: stale README baseline row; directory no longer exists")
        elif path not in missing:
            diagnostics.append(f"{path}: stale README baseline row; README.md now exists")
    for path in sorted(missing):
        if path not in baseline:
            diagnostics.append(f"{path}: missing README.md; use README_TEMPLATE.md")
    return diagnostics


def main() -> int:
    """Run module README coverage checks.

    Inputs:
    - Repository module directories and baseline file.

    Outputs:
    - Exit status 0 when README debt has not grown.
    - Exit status 1 with diagnostics when README coverage regresses.

    Transformation:
    - Discovers module directories, reads the baseline, and validates that only
      known migration debt remains undocumented.
    """

    modules = module_directories()
    baseline, diagnostics = read_baseline()
    diagnostics.extend(check_readmes(modules, baseline))
    if diagnostics:
        print("[module-readmes] failures:")
        for diagnostic in diagnostics:
            print(f"  - {diagnostic}")
        return 1
    missing_count = sum(1 for path in modules if not (ROOT / path / "README.md").is_file())
    print(f"[module-readmes] baseline enforced: {missing_count} missing README files.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
