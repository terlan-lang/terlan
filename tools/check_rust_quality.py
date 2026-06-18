#!/usr/bin/env python3
"""Check Rust file-size and inline-test quality baselines.

Inputs:
- Rust source files under `crates/`.
- `tools/quality/rust_file_size_baseline.tsv`.
- `tools/quality/rust_inline_test_baseline.txt`.

Outputs:
- Exit status 0 when current files stay within limits or existing baselines.
- Exit status 1 with path-specific diagnostics when new quality debt appears.

Transformation:
- Counts Rust source lines.
- Compares oversized files against a checked-in baseline.
    - Finds implementation files containing `#[cfg(test)]` and compares them
  against a checked-in inline-test baseline.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CRATES = ROOT / "crates"
QUALITY_DIR = ROOT / "tools" / "quality"
SIZE_BASELINE = QUALITY_DIR / "rust_file_size_baseline.tsv"
INLINE_TEST_BASELINE = QUALITY_DIR / "rust_inline_test_baseline.txt"
IMPL_LINE_LIMIT = 1000
TEST_LINE_LIMIT = 2000


@dataclass(frozen=True)
class RustFile:
    """Rust source file and measured line count.

    Inputs:
    - `path`: repository-relative Rust source path.
    - `lines`: number of text lines in the file.

    Outputs:
    - Immutable file measurement used by quality checks.

    Transformation:
    - Keeps path and measured size together so diagnostics can report both.
    """

    path: Path
    lines: int

    def limit(self) -> int:
        """Return the configured line limit for this Rust file.

        Inputs:
        - The file path.

        Outputs:
        - Test-file line limit for `*_test.rs` files.
        - Implementation-file line limit for all other Rust files.

        Transformation:
        - Classifies by filename suffix only, matching the project test layout
          rule.
        """

        if self.path.name.endswith("_test.rs"):
            return TEST_LINE_LIMIT
        return IMPL_LINE_LIMIT


def iter_rust_files() -> list[RustFile]:
    """Return measured Rust files under `crates/`.

    Inputs:
    - Repository `crates/` directory.

    Outputs:
    - Sorted Rust file measurements.

    Transformation:
    - Recursively scans `.rs` files, counts lines, and stores paths relative to
      the repository root for stable baseline matching.
    """

    files: list[RustFile] = []
    for path in sorted(CRATES.rglob("*.rs")):
        if not path.is_file():
            continue
        relative = path.relative_to(ROOT)
        lines = len(path.read_text(encoding="utf-8").splitlines())
        files.append(RustFile(path=relative, lines=lines))
    return files


def read_size_baseline() -> tuple[dict[Path, int], list[str]]:
    """Read the file-size quality baseline.

    Inputs:
    - `tools/quality/rust_file_size_baseline.tsv`.

    Outputs:
    - Mapping from repository-relative path to maximum allowed line count.
    - Diagnostics for malformed rows.

    Transformation:
    - Parses tab-separated path/count rows into typed baseline values.
    """

    baseline: dict[Path, int] = {}
    diagnostics: list[str] = []
    for number, line in enumerate(SIZE_BASELINE.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != 2:
            diagnostics.append(f"{SIZE_BASELINE}:{number}: expected path<TAB>lines")
            continue
        path_text, lines_text = fields
        try:
            baseline[Path(path_text)] = int(lines_text)
        except ValueError:
            diagnostics.append(f"{SIZE_BASELINE}:{number}: invalid line count `{lines_text}`")
    return baseline, diagnostics


def read_inline_test_baseline() -> tuple[set[Path], list[str]]:
    """Read the inline-test quality baseline.

    Inputs:
    - `tools/quality/rust_inline_test_baseline.txt`.

    Outputs:
    - Set of repository-relative paths allowed to contain `#[cfg(test)]`.
    - Diagnostics for malformed rows.

    Transformation:
    - Parses one path per line while allowing comments and blank lines.
    """

    baseline: set[Path] = set()
    diagnostics: list[str] = []
    for number, line in enumerate(INLINE_TEST_BASELINE.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        if "\t" in line:
            diagnostics.append(f"{INLINE_TEST_BASELINE}:{number}: expected one path per line")
            continue
        baseline.add(Path(line))
    return baseline, diagnostics


def has_inline_test_marker(text: str) -> bool:
    """Return whether source text contains non-adjacent inline test config.

    Inputs:
    - `text`: Rust implementation source.

    Outputs:
    - `True` when the file contains an inline `#[cfg(test)]` marker.
    - `False` when every marker belongs to an adjacent `#[path = "*_test.rs"]`
      module declaration.

    Transformation:
    - Scans source lines and treats `#[cfg(test)]` followed by optional blank
      lines and a `#[path = "..._test.rs"]` attribute as the approved adjacent
      test-module pattern.
    """

    lines = text.splitlines()
    for index, line in enumerate(lines):
        if "#[cfg(test)]" not in line:
            continue
        next_index = index + 1
        while next_index < len(lines) and not lines[next_index].strip():
            next_index += 1
        if next_index < len(lines):
            next_line = lines[next_index].strip()
            if next_line.startswith("#[path = ") and "_test.rs" in next_line:
                continue
        return True
    return False


def files_with_inline_tests(files: list[RustFile]) -> set[Path]:
    """Return implementation files that contain inline Rust test configuration.

    Inputs:
    - Measured Rust files.

    Outputs:
    - Repository-relative implementation paths containing `#[cfg(test)]`.

    Transformation:
    - Ignores adjacent `*_test.rs` test modules because those are the required
      test layout.
    - Reads implementation Rust source files and allows adjacent path-based
      test modules while rejecting other inline test configuration markers.
    """

    paths: set[Path] = set()
    for file in files:
        if file.path.name.endswith("_test.rs"):
            continue
        text = (ROOT / file.path).read_text(encoding="utf-8")
        if has_inline_test_marker(text):
            paths.add(file.path)
    return paths


def check_file_sizes(files: list[RustFile], baseline: dict[Path, int]) -> list[str]:
    """Validate line-count limits against the baseline.

    Inputs:
    - Current measured Rust files.
    - Baseline maximum line counts.

    Outputs:
    - Diagnostics for new oversized files, baseline growth, and stale baseline
      rows.

    Transformation:
    - Enforces hard limits for new files while allowing existing debt only up to
      the recorded baseline line count.
    """

    diagnostics: list[str] = []
    current = {file.path: file for file in files}

    for path in sorted(baseline):
        if path not in current:
            diagnostics.append(f"{path}: stale file-size baseline row")

    for file in files:
        limit = file.limit()
        if file.lines <= limit:
            continue
        allowed = baseline.get(file.path)
        if allowed is None:
            diagnostics.append(
                f"{file.path}: {file.lines} lines exceeds {limit}; split file or add reviewed baseline"
            )
            continue
        if file.lines > allowed:
            diagnostics.append(
                f"{file.path}: {file.lines} lines exceeds baseline {allowed}; split before adding code"
            )
    return diagnostics


def check_inline_tests(current: set[Path], baseline: set[Path]) -> list[str]:
    """Validate inline test usage against the baseline.

    Inputs:
    - Current files containing `#[cfg(test)]`.
    - Baseline files allowed to contain inline tests.

    Outputs:
    - Diagnostics for new inline test files and stale baseline rows.

    Transformation:
    - Prevents new inline-test debt while allowing current debt to be migrated
      out over time.
    """

    diagnostics: list[str] = []
    for path in sorted(baseline):
        if path not in current:
            diagnostics.append(f"{path}: stale inline-test baseline row")
    for path in sorted(current):
        if path not in baseline:
            diagnostics.append(f"{path}: new inline #[cfg(test)] block; move tests to adjacent *_test.rs")
    return diagnostics


def main() -> int:
    """Run Rust quality baseline checks.

    Inputs:
    - Repository Rust files and quality baseline files.

    Outputs:
    - Exit status 0 when quality debt has not grown.
    - Exit status 1 with diagnostics when quality debt grows or baselines are
      stale.

    Transformation:
    - Combines file-size and inline-test validation into one check target.
    """

    files = iter_rust_files()
    size_baseline, diagnostics = read_size_baseline()
    inline_baseline, inline_diagnostics = read_inline_test_baseline()
    diagnostics.extend(inline_diagnostics)
    diagnostics.extend(check_file_sizes(files, size_baseline))
    diagnostics.extend(check_inline_tests(files_with_inline_tests(files), inline_baseline))

    if diagnostics:
        print("[rust-quality] failures:")
        for diagnostic in diagnostics:
            print(f"  - {diagnostic}")
        return 1

    oversized_count = sum(1 for file in files if file.lines > file.limit())
    inline_count = len(files_with_inline_tests(files))
    print(
        f"[rust-quality] baseline enforced: {oversized_count} oversized files, "
        f"{inline_count} inline-test files."
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
