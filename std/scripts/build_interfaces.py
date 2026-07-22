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
    ".native_boundary.json",
    ".native_boundary.rs",
)


def compiler_command() -> list[str]:
    """Return the compiler command used for std summary emission.

    Inputs:
    - Optional `TERLC` environment variable.
    - Repository-local `target/debug/terlc` binary.

    Outputs:
    - Command vector that accepts std interface-generation arguments.

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
        ["cargo", "build", "-q", "-p", "terlan"],
        cwd=ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        return (result.stdout + result.stderr).rstrip()
    return None


def source_contains_compiler_native(source: Path) -> bool:
    """Return whether a source contains compiler-native declarations.

    Inputs:
    - `source`: stdlib source file.

    Output:
    - `True` when NativeBoundary metadata artifacts should be generated.

    Transformation:
    - Scans source text for `@compiler.native` annotations without parsing.
    """

    try:
        return "@compiler.native" in source.read_text(encoding="utf-8")
    except OSError:
        return False


def run_command(command: list[str]) -> subprocess.CompletedProcess[str]:
    """Run one std generation subprocess.

    Inputs:
    - `command`: compiler command vector.

    Output:
    - Completed subprocess with captured output.

    Transformation:
    - Applies stable environment defaults shared by interface and NativeBoundary
      metadata generation.
    """

    env = os.environ.copy()
    env.setdefault("CARGO_TERM_COLOR", "never")
    return subprocess.run(
        command,
        cwd=ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def run_emit(source: Path, out_dir: Path) -> str | None:
    """Emit interface metadata for one stdlib source file.

    Inputs:
    - `source`: absolute path to a `.terl` source under `std/`.
    - `out_dir`: output directory for generated summary artifacts.

    Outputs:
    - `None` when `terlc interface` and optional NativeBoundary metadata emission
      succeeds.
    - A diagnostic string when generation fails.

    Transformation:
    - Runs the local `terlc interface` command with the std summary output
      directory so std interfaces can be regenerated from source. Sources with
      compiler-native annotations also emit checked NativeBoundary metadata.
    """

    result = run_command(
        [
            *compiler_command(),
            "interface",
            str(source.relative_to(ROOT)),
            "--out-dir",
            str(out_dir),
        ]
    )
    if result.returncode != 0:
        return f"{source}: interface generation failed\n{(result.stdout + result.stderr).rstrip()}"
    if source_contains_compiler_native(source):
        result = run_command(
            [
                *compiler_command(),
                "--native-policy",
                "native_boundary_optional",
                "emit-native-metadata",
                str(source.relative_to(ROOT)),
                "--out-dir",
                str(out_dir),
            ]
        )
        if result.returncode != 0:
            return (
                f"{source}: NativeBoundary metadata generation failed\n"
                f"{(result.stdout + result.stderr).rstrip()}"
            )
    return None


def run_interface_batch(sources: list[Path], out_dir: Path) -> str | None:
    """Emit all std interfaces through one compiler process.

    Inputs:
    - `sources`: sorted release stdlib sources.
    - `out_dir`: output directory for generated summaries.

    Outputs:
    - `None` on success or a combined compiler diagnostic on failure.

    Transformation:
    - Passes every source to the compiler's batch interface command so parsing,
      embedded-interface loading, and dependency graph construction happen
      once instead of once per module.
    """

    result = run_command(
        [
            *compiler_command(),
            "interface",
            *(str(source.relative_to(ROOT)) for source in sources),
            "--out-dir",
            str(out_dir),
        ]
    )
    if result.returncode != 0:
        return (result.stdout + result.stderr).rstrip()
    return None


def run_native_emit(source: Path, out_dir: Path) -> str | None:
    """Emit NativeBoundary metadata for one compiler-native std source."""

    result = run_command(
        [
            *compiler_command(),
            "--native-policy",
            "native_boundary_optional",
            "emit-native-metadata",
            str(source.relative_to(ROOT)),
            "--out-dir",
            str(out_dir),
        ]
    )
    if result.returncode != 0:
        return (
            f"{source}: NativeBoundary metadata generation failed\n"
            f"{(result.stdout + result.stderr).rstrip()}"
        )
    return None


def is_release_summary_artifact(path: Path) -> bool:
    """Return whether a generated artifact belongs in `std/summaries`.

    Inputs:
    - `path`: generated file path inside the selected output directory.

    Outputs:
    - `True` for release-owned summary and NativeBoundary metadata artifacts.
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
    - `out_dir`: directory where interface generation wrote summary files.

    Outputs:
    - Repository-relative or absolute paths removed from `out_dir`.

    Transformation:
    - Iterates direct child files and unlinks non-release-owned artifacts while
      keeping `.typi`, `.typi.deps`, `.native_boundary.json`, and `.native_boundary.rs`.
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
        default=default_interface_jobs(),
        help="maximum parallel compiler interface jobs per dependency pass",
    )
    return parser.parse_args()


def default_interface_jobs() -> int:
    """Return the bounded stdlib interface-generation worker count.

    Inputs:
    - Optional `TERLAN_STDLIB_INTERFACE_JOBS` environment override.
    - Host logical CPU count.

    Outputs:
    - A positive worker count capped at 16 by default.

    Transformation:
    - Uses enough independent compiler processes to keep prerelease summary
      drift checks bounded without inheriting an unbounded host CPU count.
    """

    configured = os.environ.get("TERLAN_STDLIB_INTERFACE_JOBS")
    if configured is not None:
        try:
            jobs = int(configured)
        except ValueError as error:
            raise ValueError(
                "TERLAN_STDLIB_INTERFACE_JOBS must be a positive integer"
            ) from error
        if jobs < 1:
            raise ValueError(
                "TERLAN_STDLIB_INTERFACE_JOBS must be greater than zero"
            )
        return jobs
    return max(1, min(16, os.cpu_count() or 1))


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
    - Encodes the release-wide test-file naming contract in one predicate while
      keeping `Test.terl` available as the public `std.test.Test` module.
    """

    return name != "Test.terl" and name.endswith("Test.terl")


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
    - Executes independent `terlc interface` jobs concurrently.
      Dependency-order failures are returned to the caller so the existing
      retry loop can run a later pass after more summaries have been
      materialized.
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


def emit_native_pass(
    sources: list[Path], out_dir: Path, jobs: int
) -> list[tuple[Path, str]]:
    """Emit compiler-native metadata concurrently after batch interfaces."""

    failures: list[tuple[Path, str]] = []
    with ThreadPoolExecutor(max_workers=max(1, jobs)) as executor:
        futures = {
            executor.submit(run_native_emit, source, out_dir): source
            for source in sources
            if source_contains_compiler_native(source)
        }
        for future in as_completed(futures):
            source = futures[future]
            if output := future.result():
                failures.append((source, output))
    failures.sort(key=lambda item: item[0])
    return failures


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
    - Removes scratch artifacts that generators may write beside the
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
    sources.sort()
    if failure := run_interface_batch(sources, out_dir):
        failures.append(failure)
    else:
        failures.extend(
            output
            for _source, output in emit_native_pass(sources, out_dir, args.jobs)
        )

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
