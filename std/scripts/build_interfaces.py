#!/usr/bin/env python3
"""Build `.typi` summaries for `std/` modules used by Terlan checks."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor, as_completed
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


def compiler_command() -> list[str]:
    """Return the compiler command used for std summary emission.

    Inputs:
    - Optional `TERLC` environment variable.
    - Repository-local `target/debug/terlc` binary.

    Outputs:
    - Command vector that accepts `emit ...` arguments.

    Transformation:
    - Prefers an explicit compiler path from the environment and otherwise
      reuses the workspace debug binary built once by `ensure_compiler`.
    """

    configured = os.environ.get("TERLC")
    if configured:
        return [configured]
    return [str(ROOT / "target" / "debug" / "terlc")]


def ensure_compiler() -> str | None:
    """Ensure the std summary compiler binary is available.

    Inputs:
    - Repository-local Cargo workspace.
    - Optional `TERLC` environment override.

    Outputs:
    - `None` when a compiler command is ready.
    - Combined stdout/stderr text when building the compiler fails.

    Transformation:
    - Avoids running `cargo run` once per std source by compiling `terlc` once
      before the per-file emission loop.
    """

    if os.environ.get("TERLC"):
        return None
    result = subprocess.run(
        ["cargo", "build", "-q", "-p", "terlan_cli"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        return (result.stdout + result.stderr).rstrip()
    return None


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
            *compiler_command(),
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
    parser.add_argument(
        "--jobs",
        type=int,
        default=max(1, min(8, os.cpu_count() or 1)),
        help="maximum parallel compiler emit jobs per dependency pass",
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
    - Classifies by repository-relative path segments and the canonical
      `Test.terl` suffix without reading source contents.
    """

    relative_parts = path.relative_to(STD_DIR).parts
    return (
        path.is_file()
        and not is_test_source_name(path.name)
        and "summaries" not in relative_parts
        and "disabled" not in relative_parts
        and not is_generated_js_binding_source(path)
    )


def is_test_source_name(name: str) -> bool:
    """Return whether a filename is a Terlan test source.

    Inputs:
    - `name`: filesystem basename for a candidate source file.

    Output:
    - `True` when the file uses the canonical `*Test.terl` source suffix.

    Transformation:
    - Encodes the release-wide test-file naming contract in one predicate.
    """

    return name.endswith("Test.terl")


def is_generated_js_binding_source(path: Path) -> bool:
    """Return whether a source file is owned by the JS binding generator.

    Inputs:
    - `path`: candidate std source file.

    Outputs:
    - `True` for generated TypeScript-backed `std.js` binding sources.
    - `False` for hand-authored std sources.

    Transformation:
    - Reads only the leading provenance header and recognizes generated
      TypeScript standard-library bindings by their generator profile.
    """

    try:
        relative_parts = path.relative_to(STD_DIR).parts
    except ValueError:
        return False
    if len(relative_parts) < 2 or relative_parts[0] != "js":
        return False
    try:
        header = "\n".join(path.read_text(encoding="utf-8").splitlines()[:12])
    except OSError:
        return False
    return "@generated true" in header and "@generator-profile typescript-standard-js-dom" in header


def emit_pass(sources: list[Path], out_dir: Path, jobs: int) -> tuple[int, list[tuple[Path, str]]]:
    """Run one parallel std summary emission pass.

    Inputs:
    - `sources`: source files still waiting for successful summary emission.
    - `out_dir`: summary output directory.
    - `jobs`: maximum number of concurrent compiler processes.

    Outputs:
    - Count of successful source emissions.
    - Source/error pairs for files that failed in this pass.

    Transformation:
    - Executes independent `terlc emit` jobs concurrently. Dependency-order
      failures are returned to the caller so the existing retry loop can run a
      later pass after more summaries have been materialized.
    """

    emitted_count = 0
    next_pending: list[tuple[Path, str]] = []
    with ThreadPoolExecutor(max_workers=max(1, jobs)) as executor:
        futures = {
            executor.submit(run_emit, source, out_dir): source
            for source in sources
        }
        for future in as_completed(futures):
            source = futures[future]
            output = future.result()
            if output:
                next_pending.append((source, output))
            else:
                emitted_count += 1
    next_pending.sort(key=lambda item: item[0])
    return emitted_count, next_pending


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
    compiler_failure = ensure_compiler()
    if compiler_failure is not None:
        print("[build-stdlib-interfaces] failed to build terlc:", file=sys.stderr)
        print(compiler_failure, file=sys.stderr)
        return 1

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
        emitted_count, next_pending = emit_pass(pending, out_dir, args.jobs)

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
