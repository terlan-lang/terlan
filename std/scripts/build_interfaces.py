#!/usr/bin/env python3
"""Build `.typi` summaries for `std/` modules used by Terlan checks."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
STD_DIR = ROOT / "std"
OUT_DIR = STD_DIR / "summaries"
RELEASE_SUMMARY_SUFFIXES = (
    ".typi",
    ".typi.deps",
    ".safe_native.json",
    ".safe_native.rs",
)


def run_emit(source: Path, out_dir: Path) -> str | None:
    """Emit interface metadata for one stdlib source file.

    Inputs:
    - `source`: absolute path to a `.terl` source under `std/`.
    - `out_dir`: output directory for generated summary artifacts.

    Outputs:
    - `None` when `terlc emit` succeeds.
    - A diagnostic string when the emit command fails.

    Transformation:
    - Runs `cargo run -p terlan_cli -- emit` with the std summary output
      directory so std interfaces can be regenerated from source.
    """

    result = subprocess.run(
        [
            "cargo",
            "run",
            "-p",
            "terlan_cli",
            "--",
            "emit",
            str(source.relative_to(ROOT)),
            "--out-dir",
            str(out_dir),
            "--native-policy",
            "safe_native_optional",
        ],
        cwd=ROOT,
        env=os.environ.copy(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        return f"{source}: emit failed\n{(result.stdout + result.stderr).rstrip()}"
    return None


def is_release_summary_artifact(path: Path) -> bool:
    """Return whether a generated artifact belongs in `std/summaries`.

    Inputs:
    - `path`: generated file path inside the selected output directory.

    Outputs:
    - `True` for release-owned summary and SafeNative metadata artifacts.
    - `False` for backend scratch artifacts such as `.erl` and `.hrl`.

    Transformation:
    - Classifies by file suffix against the release-owned summary suffix list.
    """

    name = path.name
    if name == ".gitkeep":
        return True
    return any(name.endswith(suffix) for suffix in RELEASE_SUMMARY_SUFFIXES)


def remove_non_summary_artifacts(out_dir: Path) -> list[Path]:
    """Remove backend artifacts generated beside std summaries.

    Inputs:
    - `out_dir`: directory where `terlc emit` wrote summary and backend files.

    Outputs:
    - Repository-relative or absolute paths removed from `out_dir`.

    Transformation:
    - Iterates direct child files and unlinks non-release-owned artifacts while
      keeping `.typi`, `.typi.deps`, `.safe_native.json`, and `.safe_native.rs`.
    """

    removed: list[Path] = []
    for path in sorted(out_dir.iterdir()):
        if not path.is_file() or is_release_summary_artifact(path):
            continue
        path.unlink()
        try:
            removed.append(path.relative_to(ROOT))
        except ValueError:
            removed.append(path)
    return removed


def parse_args() -> argparse.Namespace:
    """Parse std interface generation command-line options.

    Inputs:
    - Process command-line arguments.

    Outputs:
    - Parsed namespace with the selected output directory.

    Transformation:
    - Keeps the default command mutating `std/summaries` for maintainer
      regeneration while allowing validation commands to write into temp space.
    """

    parser = argparse.ArgumentParser(description="build stdlib interface summaries")
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=OUT_DIR,
        help="directory where generated summary artifacts are written",
    )
    return parser.parse_args()


def is_std_release_source(path: Path) -> bool:
    """Return whether a std source should emit release summaries.

    Inputs:
    - `path`: candidate `.terl` file under `std/`.

    Outputs:
    - `True` when the file is a publishable std module.
    - `False` when the file is a test, summary, or disabled scratch source.

    Transformation:
    - Classifies by repository-relative path segments and the `_test.terl`
      suffix without reading source contents.
    """

    relative_parts = path.relative_to(STD_DIR).parts
    return (
        path.is_file()
        and not path.name.endswith("_test.terl")
        and "summaries" not in relative_parts
        and "disabled" not in relative_parts
    )


def main() -> int:
    """Regenerate checked-in stdlib interface summaries.

    Inputs:
    - The repository `std/` tree and local Rust/Cargo toolchain.
    - Optional `--out-dir` override for read-only drift checks.

    Outputs:
    - Exit status 0 when all selected stdlib sources emit summaries.
    - Exit status 1 when the std tree is missing or any source fails emission.

    Transformation:
    - Scans release stdlib sources and emits interface artifacts into
      the selected output directory.
    - Removes backend scratch artifacts that `terlc emit` writes beside the
      release-owned summary files.
    """

    args = parse_args()
    out_dir = args.out_dir.resolve()
    if not STD_DIR.is_dir():
        print("[build-stdlib-interfaces] std/ directory missing", file=sys.stderr)
        return 1

    out_dir.mkdir(parents=True, exist_ok=True)

    sources = [
        path
        for path in STD_DIR.rglob("*.terl")
        if is_std_release_source(path)
    ]

    failures: list[str] = []
    env = os.environ.copy()
    env.setdefault("CARGO_TERM_COLOR", "never")

    pending = sorted(sources)
    while pending:
        next_pending: list[tuple[Path, str]] = []
        emitted_count = 0

        for source in pending:
            output = run_emit(source, out_dir)
            if output:
                next_pending.append((source, output))
            else:
                emitted_count += 1

        if not next_pending:
            break

        if emitted_count == 0:
            failures = [output for _source, output in next_pending]
            break

        pending = [source for source, _output in next_pending]

    if failures:
        print("[build-stdlib-interfaces] failures:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    removed = remove_non_summary_artifacts(out_dir)
    print(f"[build-stdlib-interfaces] wrote {len(sources)} interfaces to {out_dir}")
    if removed:
        print(f"[build-stdlib-interfaces] removed {len(removed)} backend artifacts")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
