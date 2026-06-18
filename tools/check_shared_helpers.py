#!/usr/bin/env python3
"""Check cross-module Rust helper duplication against a migration baseline.

Inputs:
- Hand-authored Rust implementation files under `crates/`.
- `tools/quality/rust_duplicate_helper_baseline.tsv`.

Outputs:
- Exit status 0 when duplicate helper bodies match or improve on the baseline.
- Exit status 1 with diagnostics for new, grown, or stale duplicate groups.

Transformation:
- Extracts Rust function bodies with a conservative balanced-brace scanner.
- Normalizes comments and whitespace inside each body.
- Hashes bodies above the configured size threshold.
- Compares cross-file duplicate body groups against a checked-in baseline.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
from pathlib import Path
import re


ROOT = Path(__file__).resolve().parents[1]
CRATES = ROOT / "crates"
QUALITY_DIR = ROOT / "tools" / "quality"
BASELINE = QUALITY_DIR / "rust_duplicate_helper_baseline.tsv"
MIN_LOGICAL_LINES = 12
FUNCTION_PATTERN = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\b")


@dataclass(frozen=True)
class FunctionBody:
    """Rust function body extracted from an implementation source file.

    Inputs:
    - `path`: repository-relative Rust implementation path.
    - `name`: declared function name.
    - `line`: one-based function declaration line.
    - `body`: raw text between the function body's braces.

    Outputs:
    - Immutable function-body record used for duplicate-helper detection.

    Transformation:
    - Keeps source identity and body text together so diagnostics can point to
      the duplicated helpers that should be extracted.
    """

    path: Path
    name: str
    line: int
    body: str

    def location(self) -> str:
        """Return a stable diagnostic location for this function body.

        Inputs:
        - Repository-relative path, line, and function name.

        Outputs:
        - Human-readable `path:line:name` location text.

        Transformation:
        - Joins source identity fields without embedding body contents.
        """

        return f"{self.path}:{self.line}:{self.name}"


@dataclass(frozen=True)
class DuplicateGroup:
    """Cross-file duplicate helper body group.

    Inputs:
    - `body_hash`: stable SHA-256 prefix for the normalized body.
    - `logical_lines`: number of normalized nonblank body lines.
    - `functions`: function bodies sharing the same normalized body.

    Outputs:
    - Immutable duplicate group used for baseline validation.

    Transformation:
    - Groups identical helper implementations only when they appear in more
      than one Rust source file.
    """

    body_hash: str
    logical_lines: int
    functions: tuple[FunctionBody, ...]

    def file_count(self) -> int:
        """Return the number of files represented by this duplicate group.

        Inputs:
        - Group function locations.

        Outputs:
        - Count of distinct repository-relative source paths.

        Transformation:
        - Deduplicates paths while preserving the body-group identity.
        """

        return len({function.path for function in self.functions})

    def occurrence_count(self) -> int:
        """Return the number of duplicated function bodies in the group.

        Inputs:
        - Group function list.

        Outputs:
        - Count of function bodies with the same normalized implementation.

        Transformation:
        - Counts occurrences directly from the extracted function records.
        """

        return len(self.functions)

    def sample_locations(self) -> str:
        """Return compact sample locations for baseline and diagnostics.

        Inputs:
        - Group function locations.

        Outputs:
        - Semicolon-separated source locations.

        Transformation:
        - Sorts locations so generated baselines are deterministic.
        """

        return ";".join(sorted(function.location() for function in self.functions))

    def baseline_row(self) -> str:
        """Return this duplicate group as a baseline row.

        Inputs:
        - Group hash, logical line count, occurrence count, and sample
          locations.

        Outputs:
        - Tab-separated baseline row.

        Transformation:
        - Serializes the reviewed duplicate group into stable text.
        """

        return (
            f"{self.body_hash}\t{self.logical_lines}\t"
            f"{self.occurrence_count()}\t{self.sample_locations()}"
        )


@dataclass(frozen=True)
class BaselineGroup:
    """Reviewed duplicate helper baseline entry.

    Inputs:
    - `body_hash`: hash of a reviewed duplicate body.
    - `logical_lines`: reviewed normalized body length.
    - `occurrences`: maximum allowed occurrence count.
    - `samples`: diagnostic sample text from the baseline file.

    Outputs:
    - Immutable baseline entry used to detect new or stale duplication debt.

    Transformation:
    - Represents one tab-separated baseline row as typed values.
    """

    body_hash: str
    logical_lines: int
    occurrences: int
    samples: str


def line_number_at(text: str, index: int) -> int:
    """Return the one-based line number for a byte-offset-like string index.

    Inputs:
    - Full source text.
    - Character index inside the source text.

    Outputs:
    - One-based source line number.

    Transformation:
    - Counts newline characters before the index.
    """

    return text.count("\n", 0, index) + 1


def strip_comments(text: str) -> str:
    """Return Rust-like text with comments removed.

    Inputs:
    - Raw function body text.

    Outputs:
    - Function body text without line or block comments.

    Transformation:
    - Scans character-by-character to avoid stripping comment markers inside
      string and character literals.
    """

    output: list[str] = []
    index = 0
    block_depth = 0
    in_line_comment = False
    in_string = False
    in_char = False
    escaped = False

    while index < len(text):
        current = text[index]
        next_char = text[index + 1] if index + 1 < len(text) else ""

        if in_line_comment:
            if current == "\n":
                in_line_comment = False
                output.append(current)
            index += 1
            continue

        if block_depth > 0:
            if current == "/" and next_char == "*":
                block_depth += 1
                index += 2
                continue
            if current == "*" and next_char == "/":
                block_depth -= 1
                index += 2
                continue
            if current == "\n":
                output.append(current)
            index += 1
            continue

        if in_string:
            output.append(current)
            if escaped:
                escaped = False
            elif current == "\\":
                escaped = True
            elif current == '"':
                in_string = False
            index += 1
            continue

        if in_char:
            output.append(current)
            if escaped:
                escaped = False
            elif current == "\\":
                escaped = True
            elif current == "'":
                in_char = False
            index += 1
            continue

        if current == "/" and next_char == "/":
            in_line_comment = True
            index += 2
            continue
        if current == "/" and next_char == "*":
            block_depth = 1
            index += 2
            continue
        if current == '"':
            in_string = True
            output.append(current)
            index += 1
            continue
        if current == "'":
            in_char = True
            output.append(current)
            index += 1
            continue

        output.append(current)
        index += 1

    return "".join(output)


def skip_string_or_comment(
    text: str,
    index: int,
    in_line_comment: bool,
    block_depth: int,
    in_string: bool,
    in_char: bool,
    escaped: bool,
) -> tuple[int, bool, int, bool, bool, bool, bool]:
    """Advance one scanner step through non-structural Rust text.

    Inputs:
    - Source text and current index.
    - Current line-comment, block-comment, string, char, and escape states.

    Outputs:
    - Updated index and scanner states.
    - Boolean flag indicating whether structural brace handling should be
      skipped for the consumed character.

    Transformation:
    - Handles comments and literals so brace matching ignores their contents.
    """

    current = text[index]
    next_char = text[index + 1] if index + 1 < len(text) else ""

    if in_line_comment:
        return index + 1, current != "\n", block_depth, in_string, in_char, False, True
    if block_depth > 0:
        if current == "/" and next_char == "*":
            return index + 2, in_line_comment, block_depth + 1, in_string, in_char, escaped, True
        if current == "*" and next_char == "/":
            return index + 2, in_line_comment, block_depth - 1, in_string, in_char, escaped, True
        return index + 1, in_line_comment, block_depth, in_string, in_char, escaped, True
    if in_string:
        if escaped:
            return index + 1, in_line_comment, block_depth, in_string, in_char, False, True
        if current == "\\":
            return index + 1, in_line_comment, block_depth, in_string, in_char, True, True
        if current == '"':
            return index + 1, in_line_comment, block_depth, False, in_char, False, True
        return index + 1, in_line_comment, block_depth, in_string, in_char, escaped, True
    if in_char:
        if escaped:
            return index + 1, in_line_comment, block_depth, in_string, in_char, False, True
        if current == "\\":
            return index + 1, in_line_comment, block_depth, in_string, in_char, True, True
        if current == "'":
            return index + 1, in_line_comment, block_depth, in_string, False, False, True
        return index + 1, in_line_comment, block_depth, in_string, in_char, escaped, True
    if current == "/" and next_char == "/":
        return index + 2, True, block_depth, in_string, in_char, escaped, True
    if current == "/" and next_char == "*":
        return index + 2, in_line_comment, 1, in_string, in_char, escaped, True
    if current == '"':
        return index + 1, in_line_comment, block_depth, True, in_char, escaped, True
    if current == "'":
        return index + 1, in_line_comment, block_depth, in_string, True, escaped, True
    return index, in_line_comment, block_depth, in_string, in_char, escaped, False


def find_body_end(text: str, open_brace: int) -> int | None:
    """Return the closing brace index for a Rust function body.

    Inputs:
    - Source text.
    - Index of the function body's opening brace.

    Outputs:
    - Closing brace index when a balanced body is found.
    - `None` when the body is malformed or incomplete.

    Transformation:
    - Scans with balanced braces while ignoring braces inside comments and
      string or character literals.
    """

    depth = 0
    index = open_brace
    in_line_comment = False
    block_depth = 0
    in_string = False
    in_char = False
    escaped = False

    while index < len(text):
        (
            next_index,
            in_line_comment,
            block_depth,
            in_string,
            in_char,
            escaped,
            skipped,
        ) = skip_string_or_comment(
            text,
            index,
            in_line_comment,
            block_depth,
            in_string,
            in_char,
            escaped,
        )
        if skipped:
            index = next_index
            continue

        current = text[index]
        if current == "{":
            depth += 1
        elif current == "}":
            depth -= 1
            if depth == 0:
                return index
        index += 1
    return None


def extract_function_bodies(path: Path) -> list[FunctionBody]:
    """Extract function bodies from one Rust implementation file.

    Inputs:
    - Absolute Rust source path.

    Outputs:
    - Function body records with repository-relative paths.

    Transformation:
    - Finds function declarations, skips trait/interface signatures ending in
      semicolons, and extracts balanced brace bodies.
    """

    text = path.read_text(encoding="utf-8")
    relative = path.relative_to(ROOT)
    bodies: list[FunctionBody] = []
    for match in FUNCTION_PATTERN.finditer(text):
        open_brace = text.find("{", match.end())
        semicolon = text.find(";", match.end())
        if open_brace < 0 or (semicolon >= 0 and semicolon < open_brace):
            continue
        close_brace = find_body_end(text, open_brace)
        if close_brace is None:
            continue
        bodies.append(
            FunctionBody(
                path=relative,
                name=match.group(1),
                line=line_number_at(text, match.start()),
                body=text[open_brace + 1 : close_brace],
            )
        )
    return bodies


def normalize_body(body: str) -> tuple[str, int]:
    """Return normalized function body text and logical line count.

    Inputs:
    - Raw function body text.

    Outputs:
    - Whitespace-normalized body text.
    - Count of nonblank normalized body lines.

    Transformation:
    - Removes comments, trims lines, discards blank lines, and joins the
      remaining body lines deterministically.
    """

    lines = [line.strip() for line in strip_comments(body).splitlines()]
    logical_lines = [line for line in lines if line]
    return "\n".join(logical_lines), len(logical_lines)


def duplicate_groups() -> list[DuplicateGroup]:
    """Return current cross-file duplicate helper groups.

    Inputs:
    - Rust implementation files under `crates/`.

    Outputs:
    - Duplicate groups above `MIN_LOGICAL_LINES`.

    Transformation:
    - Hashes normalized bodies and keeps only groups that occur in more than
      one source file.
    """

    grouped: dict[str, tuple[int, list[FunctionBody]]] = {}
    for path in sorted(CRATES.rglob("*.rs")):
        if path.name.endswith("_test.rs"):
            continue
        for body in extract_function_bodies(path):
            normalized, logical_lines = normalize_body(body.body)
            if logical_lines < MIN_LOGICAL_LINES:
                continue
            body_hash = hashlib.sha256(normalized.encode("utf-8")).hexdigest()[:16]
            if body_hash not in grouped:
                grouped[body_hash] = (logical_lines, [])
            grouped[body_hash][1].append(body)

    duplicates: list[DuplicateGroup] = []
    for body_hash, (logical_lines, functions) in grouped.items():
        if len({function.path for function in functions}) <= 1:
            continue
        duplicates.append(DuplicateGroup(body_hash, logical_lines, tuple(functions)))
    return sorted(duplicates, key=lambda group: group.body_hash)


def read_baseline() -> tuple[dict[str, BaselineGroup], list[str]]:
    """Read the duplicate-helper baseline file.

    Inputs:
    - `tools/quality/rust_duplicate_helper_baseline.tsv`.

    Outputs:
    - Mapping from body hash to baseline entry.
    - Diagnostics for malformed rows.

    Transformation:
    - Parses tab-separated hash, logical-line count, occurrence count, and
      sample location rows.
    """

    baseline: dict[str, BaselineGroup] = {}
    diagnostics: list[str] = []
    for number, line in enumerate(BASELINE.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) != 4:
            diagnostics.append(f"{BASELINE}:{number}: expected hash<TAB>lines<TAB>count<TAB>samples")
            continue
        body_hash, line_count_text, occurrence_text, samples = fields
        try:
            line_count = int(line_count_text)
            occurrences = int(occurrence_text)
        except ValueError:
            diagnostics.append(f"{BASELINE}:{number}: line count and occurrence count must be integers")
            continue
        baseline[body_hash] = BaselineGroup(body_hash, line_count, occurrences, samples)
    return baseline, diagnostics


def compare_to_baseline(groups: list[DuplicateGroup], baseline: dict[str, BaselineGroup]) -> list[str]:
    """Validate current duplicate groups against the reviewed baseline.

    Inputs:
    - Current duplicate helper groups.
    - Reviewed duplicate helper baseline.

    Outputs:
    - Diagnostics for new duplicate groups, grown groups, changed body sizes,
      and stale baseline entries.

    Transformation:
    - Uses body hashes as stable reviewed-debt identifiers and occurrence
      counts as the maximum allowed duplication for each hash.
    """

    diagnostics: list[str] = []
    current = {group.body_hash: group for group in groups}

    for body_hash in sorted(baseline):
        if body_hash not in current:
            diagnostics.append(f"{BASELINE}: stale duplicate-helper baseline row `{body_hash}`")

    for group in groups:
        expected = baseline.get(group.body_hash)
        if expected is None:
            diagnostics.append(
                f"new duplicate helper body {group.body_hash} appears "
                f"{group.occurrence_count()} times: {group.sample_locations()}"
            )
            continue
        if group.logical_lines != expected.logical_lines:
            diagnostics.append(
                f"duplicate helper body {group.body_hash} changed from "
                f"{expected.logical_lines} to {group.logical_lines} logical lines"
            )
        if group.occurrence_count() > expected.occurrences:
            diagnostics.append(
                f"duplicate helper body {group.body_hash} appears "
                f"{group.occurrence_count()} times, baseline allows {expected.occurrences}: "
                f"{group.sample_locations()}"
            )
    return diagnostics


def write_baseline(groups: list[DuplicateGroup]) -> None:
    """Write the current duplicate-helper groups as the reviewed baseline.

    Inputs:
    - Current duplicate helper groups.

    Outputs:
    - Updated `rust_duplicate_helper_baseline.tsv` file.

    Transformation:
    - Serializes current groups with explanatory comments for review.
    """

    QUALITY_DIR.mkdir(parents=True, exist_ok=True)
    lines = [
        "# Reviewed cross-file duplicate Rust helper bodies.",
        "# Format: body_hash<TAB>logical_lines<TAB>occurrences<TAB>sample_locations",
        "# Existing rows are migration debt; new or grown groups must be extracted into shared modules.",
    ]
    lines.extend(group.baseline_row() for group in groups)
    BASELINE.write_text("\n".join(lines) + "\n", encoding="utf-8")


def parse_args() -> argparse.Namespace:
    """Parse duplicate-helper checker command-line arguments.

    Inputs:
    - Process command-line arguments.

    Outputs:
    - Parsed namespace with checker options.

    Transformation:
    - Defines a maintainer-only baseline regeneration flag.
    """

    parser = argparse.ArgumentParser(description="check Rust duplicate helper baseline")
    parser.add_argument(
        "--write-baseline",
        action="store_true",
        help="rewrite the reviewed duplicate-helper baseline from current source",
    )
    return parser.parse_args()


def main() -> int:
    """Run the duplicate-helper quality gate.

    Inputs:
    - Current Rust implementation files.
    - Optional baseline regeneration flag.

    Outputs:
    - Exit status 0 when duplicate-helper debt is stable or baseline is written.
    - Exit status 1 when new, grown, stale, or malformed duplicate baseline
      state is found.

    Transformation:
    - Discovers duplicate helper bodies, optionally writes the baseline, and
      otherwise validates current state against the baseline.
    """

    args = parse_args()
    groups = duplicate_groups()
    if args.write_baseline:
        write_baseline(groups)
        print(f"[shared-helpers] wrote baseline with {len(groups)} duplicate helper groups")
        return 0

    baseline, diagnostics = read_baseline()
    diagnostics.extend(compare_to_baseline(groups, baseline))
    if diagnostics:
        print("shared-helper-check failed:")
        for diagnostic in diagnostics:
            print(f"  - {diagnostic}")
        return 1
    print(f"[shared-helpers] baseline enforced: {len(groups)} duplicate helper groups")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
