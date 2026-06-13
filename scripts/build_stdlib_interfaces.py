#!/usr/bin/env python3
"""Generate checked-in Terlan stdlib interface summaries.

Inputs:
- `std/**/*.tl` source files in the repository.
- The current `terlc check` implementation.

Outputs:
- `std/summaries/<module>.typi` for every std source module.
- `std/summaries/<module>.typi.deps` for every std source module.
- Package index summaries such as `std/summaries/std.core.typi`.

Transformation:
- Runs each source file through `terlc check --cache-dir`, then copies the
  generated compiler interface cache into the committed std summary directory.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
import tempfile
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
STD_ROOT = REPO_ROOT / "std"
SUMMARY_DIR = STD_ROOT / "summaries"


def std_source_files() -> list[Path]:
    """Return canonical std source files that must produce summaries.

    Inputs:
    - Repository `std` tree.

    Outputs:
    - Sorted `.tl` files below `std`.

    Transformation:
    - Filters generated summaries and non-source files out of the walk while
      keeping package modules in deterministic path order.
    """
    return sorted(STD_ROOT.glob("**/*.tl"))


def generated_summary_files(cache_dir: Path) -> list[Path]:
    """Return generated summary artifacts from a cache directory.

    Inputs:
    - `cache_dir` produced by `terlc check --cache-dir`.

    Outputs:
    - Sorted `.typi` and `.typi.deps` files.

    Transformation:
    - Selects only compiler summary artifacts, leaving any unrelated temp files
      out of the committed copy step.
    """
    files = list(cache_dir.glob("*.typi")) + list(cache_dir.glob("*.typi.deps"))
    return sorted(files)


def run_terlc_check(source: Path, cache_dir: Path) -> None:
    """Generate interface cache artifacts for one std source file.

    Inputs:
    - `source`: std `.tl` file to check.
    - `cache_dir`: temporary output directory for generated summaries.

    Outputs:
    - Raises `CalledProcessError` when `terlc check` fails.

    Transformation:
    - Invokes the current compiler through Cargo so generation always reflects
      the local source tree under test.
    """
    subprocess.run(
        [
            "cargo",
            "run",
            "--quiet",
            "--locked",
            "--bin",
            "terlc",
            "--",
            "--cache-dir",
            str(cache_dir),
            "check",
            str(source.relative_to(REPO_ROOT)),
        ],
        cwd=REPO_ROOT,
        check=True,
    )


def generate_package_indexes(cache_dir: Path) -> None:
    """Generate package index summaries from direct module summaries.

    Inputs:
    - `cache_dir` containing generated `<module>.typi` files.

    Outputs:
    - Package summary files for each package with direct child modules.

    Transformation:
    - Reads generated module names, groups them by parent package, and writes
      deterministic `module Child` entries.
    """
    package_children: dict[str, set[str]] = {}
    for summary in cache_dir.glob("std.*.typi"):
        module_name = summary.stem
        parts = module_name.split(".")
        if len(parts) < 3:
            continue
        package = ".".join(parts[:-1])
        child = parts[-1]
        package_children.setdefault(package, set()).add(child)

    for package, children in sorted(package_children.items()):
        lines = [package]
        lines.extend(f"module {child}" for child in sorted(children))
        (cache_dir / f"{package}.typi").write_text("\n".join(lines) + "\n", encoding="utf-8")


def replace_committed_summaries(cache_dir: Path) -> None:
    """Replace committed generated summary artifacts with cache outputs.

    Inputs:
    - `cache_dir`: generated summary cache.
    - `SUMMARY_DIR`: committed std summary directory.

    Outputs:
    - Updated summary artifacts on disk.

    Transformation:
    - Removes old generated `.typi` and `.typi.deps` artifacts, preserves
      human documentation files, then copies freshly generated artifacts in.
    """
    SUMMARY_DIR.mkdir(parents=True, exist_ok=True)
    for old in list(SUMMARY_DIR.glob("*.typi")) + list(SUMMARY_DIR.glob("*.typi.deps")):
        old.unlink()
    for generated in generated_summary_files(cache_dir):
        shutil.copy2(generated, SUMMARY_DIR / generated.name)


def main() -> int:
    """Run std summary generation.

    Inputs:
    - Command-line invocation with no arguments.

    Outputs:
    - Process exit code `0` on success.

    Transformation:
    - Generates all direct module summaries into a temporary cache, derives
      package indexes, and promotes generated artifacts into `std/summaries`.
    """
    sources = std_source_files()
    if not sources:
        print("no std source files found", file=sys.stderr)
        return 1

    with tempfile.TemporaryDirectory(prefix="terlan-stdlib-summaries-") as tmp:
        cache_dir = Path(tmp)
        for source in sources:
            run_terlc_check(source, cache_dir)
        generate_package_indexes(cache_dir)
        replace_committed_summaries(cache_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
