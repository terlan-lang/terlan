#!/usr/bin/env python3
"""Check that SQL form analysis stays a conservative compiler boundary.

Inputs:
- `crates/terlan/src/compiler/typeck/sql_forms/README.md`.
- `crates/terlan/src/compiler/typeck/sql_forms/scanner.rs`.
- `crates/terlan/src/compiler/typeck/sql_forms/projection.rs`.

Outputs:
- Exit status 0 when SQL scanner/projection code remains documented as
  non-authoritative helper logic.
- Exit status 1 with stable diagnostics when the boundary markers are missing
  or obvious hand-rolled parser ownership markers appear.

Transformation:
- Requires explicit documentation that projection extraction is not an
  authoritative SQL parser.
- Requires scanner/projection implementation docs to describe conservative
  metadata extraction.
- Rejects local AST/parser-type names that would indicate the scanner is
  becoming a custom SQL parser instead of a narrow compiler aid.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
SQL_FORMS_ROOT = ROOT / "crates" / "terlan" / "src" / "compiler" / "typeck" / "sql_forms"
README = SQL_FORMS_ROOT / "README.md"
SCANNER = SQL_FORMS_ROOT / "scanner.rs"
PROJECTION = SQL_FORMS_ROOT / "projection.rs"

REQUIRED_MARKERS = (
    (
        README,
        "not an authoritative SQL",
    ),
    (
        SCANNER,
        "Lowercase word-like and number-like SQL tokens.",
    ),
    (
        PROJECTION,
        "Postgres-backed validation",
    ),
)

FORBIDDEN_PATTERNS = (
    re.compile(r"\bstruct\s+Sql(?:Ast|Parser|Statement|Expression|Node)\b"),
    re.compile(r"\benum\s+Sql(?:Ast|Parser|Statement|Expression|Node)\b"),
    re.compile(r"\bparse_sql_(?:statement|expression|ast|node)\b"),
    re.compile(r"\bvalidate_sql_(?:syntax|semantics)\b"),
)


@dataclass(frozen=True)
class Finding:
    """SQL boundary finding.

    Inputs:
    - `path`: repository-relative path to the file that owns the finding.
    - `line`: optional one-based line number.
    - `message`: stable diagnostic text.

    Outputs:
    - Immutable diagnostic record.

    Transformation:
    - Keeps source location and explanation together for deterministic output.
    """

    path: Path
    line: int | None
    message: str

    def render(self) -> str:
        """Return a stable diagnostic line.

        Inputs:
        - Finding path, optional line, and message.

        Outputs:
        - `path: message` or `path:line: message`.

        Transformation:
        - Formats line-aware findings without exposing unrelated source text.
        """

        if self.line is None:
            return f"{self.path}: {self.message}"
        return f"{self.path}:{self.line}: {self.message}"


def relative(path: Path) -> Path:
    """Return a repository-relative path.

    Inputs:
    - Absolute repository path.

    Outputs:
    - Path relative to `ROOT`.

    Transformation:
    - Normalizes checker output across workstations and CI paths.
    """

    return path.relative_to(ROOT)


def read_text(path: Path) -> str:
    """Read one UTF-8 source file.

    Inputs:
    - Existing repository path.

    Outputs:
    - File contents as text.

    Transformation:
    - Uses explicit UTF-8 decoding because all checked files are source files.
    """

    return path.read_text(encoding="utf-8")


def marker_findings() -> list[Finding]:
    """Return missing SQL boundary marker findings.

    Inputs:
    - Files and marker text from `REQUIRED_MARKERS`.

    Outputs:
    - Finding records for missing documentation or implementation markers.

    Transformation:
    - Scans exact marker text so the checker locks the current documented
      non-authoritative SQL boundary.
    """

    findings: list[Finding] = []
    for path, marker in REQUIRED_MARKERS:
        if marker not in read_text(path):
            findings.append(
                Finding(
                    path=relative(path),
                    line=None,
                    message=f"missing SQL boundary marker `{marker}`",
                )
            )
    return findings


def forbidden_pattern_findings() -> list[Finding]:
    """Return hand-rolled SQL parser ownership findings.

    Inputs:
    - Rust source files under `sql_forms`.

    Outputs:
    - Finding records for names that imply a local SQL AST/parser/validator.

    Transformation:
    - Searches line by line so violations identify the exact regression while
      allowing the existing scanner helpers for token masking and metadata.
    """

    findings: list[Finding] = []
    for path in sorted(SQL_FORMS_ROOT.glob("*.rs")):
        for line_no, line in enumerate(read_text(path).splitlines(), 1):
            for pattern in FORBIDDEN_PATTERNS:
                if pattern.search(line):
                    findings.append(
                        Finding(
                            path=relative(path),
                            line=line_no,
                            message="SQL forms must not grow an authoritative hand-rolled parser",
                        )
                    )
    return findings


def check_sql_form_boundary() -> list[Finding]:
    """Return all SQL form boundary findings.

    Inputs:
    - SQL form docs and implementation files.

    Outputs:
    - Finding records for every boundary violation.

    Transformation:
    - Combines required marker checks with forbidden parser-ownership scans.
    """

    return marker_findings() + forbidden_pattern_findings()


def main() -> int:
    """Run the SQL form boundary checker.

    Inputs:
    - Repository files addressed by module constants.

    Outputs:
    - Process exit code.

    Transformation:
    - Prints stable diagnostics for findings and a compact success message when
      the SQL form boundary holds.
    """

    findings = check_sql_form_boundary()
    if findings:
        for finding in findings:
            print(finding.render())
        return 1
    print("SQL form boundary OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
