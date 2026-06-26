#!/usr/bin/env python3
"""Check that DB command execution stays behind maintained adapter boundaries.

Inputs:
- `crates/terlan_cli/src/commands/db/README.md`.
- Rust source files under `crates/terlan_cli/src/commands/db`.

Outputs:
- Exit status 0 when DB command code has no direct process-backed Postgres path.
- Exit status 1 with stable diagnostics when DB command modules grow direct
  process execution or lose the documented maintained-adapter markers.

Transformation:
- Requires the DB README to name the maintained Rust/Tokio Postgres adapter.
- Rejects `std::process::Command`, `Command::new("psql")`, and checked-in
  `psql.rs` so DB commands cannot regress to external database tooling.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
DB_ROOT = ROOT / "crates" / "terlan_cli" / "src" / "commands" / "db"
DB_README = DB_ROOT / "README.md"
DB_PSQL = DB_ROOT / "psql.rs"

REQUIRED_BOUNDARY_MARKERS = (
    (
        DB_README,
        "maintained Rust/Tokio Postgres adapter",
    ),
)

FORBIDDEN_DB_PROCESS_PATTERNS = (
    re.compile(r"\buse\s+std::process::Command\b"),
    re.compile(r"\bCommand::new\s*\(\s*\"psql\"\s*\)"),
)


@dataclass(frozen=True)
class Finding:
    """DB command boundary finding.

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
        - Formats line-aware findings without exposing unrelated file content.
        """

        if self.line is None:
            return f"{self.path}: {self.message}"
        return f"{self.path}:{self.line}: {self.message}"


def relative(path: Path) -> Path:
    """Return a repository-relative path.

    Inputs:
    - Absolute path inside the repository.

    Outputs:
    - Path relative to `ROOT`.

    Transformation:
    - Normalizes checker diagnostics across local and CI workspaces.
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


def db_source_files() -> list[Path]:
    """Return production DB command Rust source files.

    Inputs:
    - `crates/terlan_cli/src/commands/db`.

    Outputs:
    - Sorted Rust implementation files excluding tests.

    Transformation:
    - Keeps tests out of forbidden-pattern scanning because tests may mention
      process boundary text while production code must route through the
      maintained adapter.
    """

    return [
        path
        for path in sorted(DB_ROOT.glob("*.rs"))
        if not path.name.endswith("_test.rs")
    ]


def marker_findings() -> list[Finding]:
    """Return missing DB boundary marker findings.

    Inputs:
    - Files and marker text from `REQUIRED_BOUNDARY_MARKERS`.

    Outputs:
    - Finding records for missing documentation or implementation markers.

    Transformation:
    - Scans exact marker text so the checker locks the current documented
      maintained Rust/Tokio adapter boundary.
    """

    findings: list[Finding] = []
    for path, marker in REQUIRED_BOUNDARY_MARKERS:
        if not path.exists():
            findings.append(
                Finding(
                    path=relative(path),
                    line=None,
                    message="missing DB command boundary file",
                )
            )
            continue
        if marker not in read_text(path):
            findings.append(
                Finding(
                    path=relative(path),
                    line=None,
                    message=f"missing DB command boundary marker `{marker}`",
                )
            )
    return findings


def forbidden_process_findings() -> list[Finding]:
    """Return direct DB process execution findings.

    Inputs:
    - Production DB command Rust source files.

    Outputs:
    - Finding records for direct database process execution markers.

    Transformation:
    - Searches line by line so direct database process regressions point at
      the exact source location.
    """

    findings: list[Finding] = []
    if DB_PSQL.exists():
        findings.append(
            Finding(
                path=relative(DB_PSQL),
                line=None,
                message="DB commands must use the maintained Rust/Tokio Postgres adapter, not db/psql.rs",
            )
        )
    for path in db_source_files():
        for line_no, line in enumerate(read_text(path).splitlines(), 1):
            for pattern in FORBIDDEN_DB_PROCESS_PATTERNS:
                if pattern.search(line):
                    findings.append(
                        Finding(
                            path=relative(path),
                            line=line_no,
                            message="DB command process execution must use the maintained Rust/Tokio Postgres adapter",
                        )
                    )
    return findings


def check_db_command_boundary() -> list[Finding]:
    """Return all DB command boundary findings.

    Inputs:
    - DB command docs and production Rust source files.

    Outputs:
    - Finding records for every DB command boundary violation.

    Transformation:
    - Combines required maintained-adapter marker checks with direct process
      execution scans.
    """

    return marker_findings() + forbidden_process_findings()


def main() -> int:
    """Run the DB command boundary checker.

    Inputs:
    - Repository files addressed by module constants.

    Outputs:
    - Process exit code.

    Transformation:
    - Prints stable diagnostics for findings and a compact success message when
      the maintained DB command boundary holds.
    """

    findings = check_db_command_boundary()
    if findings:
        for finding in findings:
            print(finding.render())
        return 1
    print("DB command maintained adapter boundary OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
