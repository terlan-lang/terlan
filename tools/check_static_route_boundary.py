#!/usr/bin/env python3
"""Check that static route parsing stays behind the formal syntax boundary.

Inputs:
- `crates/terlan/src/commands/static_site/README.md`.
- `crates/terlan/src/commands/static_site/routes.rs`.

Outputs:
- Exit status 0 when static route discovery is documented and implemented as
  a bridge from formal syntax output.
- Exit status 1 with stable diagnostics when the route module starts owning
  source-file parsing or loses its formal syntax-output boundary markers.

Transformation:
- Requires route discovery to accept `SyntaxModuleOutput`.
- Requires route declarations to be read from `SyntaxDeclarationPayload::Config`.
- Rejects filesystem source reads and direct parser entry points inside the
  static route module.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
STATIC_SITE_ROOT = ROOT / "crates" / "terlan" / "src" / "commands" / "static_site"
README = STATIC_SITE_ROOT / "README.md"
ROUTES = STATIC_SITE_ROOT / "routes.rs"

REQUIRED_BOUNDARY_MARKERS = (
    (
        README,
        "Static route declarations are accepted only after the formal Terlan parser",
    ),
    (
        ROUTES,
        "use terlan_syntax::{SyntaxDeclarationPayload, SyntaxModuleOutput};",
    ),
    (
        ROUTES,
        "pub(crate) fn discover_syntax_static_routes(",
    ),
    (
        ROUTES,
        "module: &SyntaxModuleOutput",
    ),
    (
        ROUTES,
        "SyntaxDeclarationPayload::Config",
    ),
    (
        ROUTES,
        "parser-preserved `static route` or `static routes` text",
    ),
)

FORBIDDEN_ROUTE_SOURCE_PATTERNS = (
    re.compile(r"\bfs::read_to_string\b"),
    re.compile(r"\bstd::fs::read_to_string\b"),
    re.compile(r"\bFile::open\b"),
    re.compile(r"\bread_to_string\s*\("),
    re.compile(r"\bparse_module_as_syntax_output\b"),
    re.compile(r"\bparse_module\b"),
    re.compile(r"\blex_module\b"),
)


@dataclass(frozen=True)
class Finding:
    """Static route boundary finding.

    Inputs:
    - `path`: repository-relative file path.
    - `line`: optional one-based line number.
    - `message`: stable diagnostic text.

    Outputs:
    - Immutable diagnostic record.

    Transformation:
    - Keeps source location and message together for deterministic checker
      output.
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
    - Absolute path inside the repository.

    Outputs:
    - Path relative to `ROOT`.

    Transformation:
    - Normalizes diagnostics across local and CI workspaces.
    """

    return path.relative_to(ROOT)


def read_text(path: Path) -> str:
    """Read one UTF-8 source file.

    Inputs:
    - Existing repository path.

    Outputs:
    - File contents as text.

    Transformation:
    - Uses explicit UTF-8 decoding because the checked files are source files.
    """

    return path.read_text(encoding="utf-8")


def marker_findings() -> list[Finding]:
    """Return missing static route boundary marker findings.

    Inputs:
    - Files and marker text from `REQUIRED_BOUNDARY_MARKERS`.

    Outputs:
    - Finding records for missing documentation or implementation markers.

    Transformation:
    - Scans exact marker text so the checker locks the current formal
      syntax-output ownership boundary.
    """

    findings: list[Finding] = []
    for path, marker in REQUIRED_BOUNDARY_MARKERS:
        if marker not in read_text(path):
            findings.append(
                Finding(
                    path=relative(path),
                    line=None,
                    message=f"missing static route boundary marker `{marker}`",
                )
            )
    return findings


def forbidden_source_parser_findings() -> list[Finding]:
    """Return source-parser ownership findings in the route module.

    Inputs:
    - Static route implementation file.

    Outputs:
    - Finding records for filesystem reads or direct formal parser calls.

    Transformation:
    - Searches route implementation text line by line so regressions identify
      the exact source location.
    """

    findings: list[Finding] = []
    for line_no, line in enumerate(read_text(ROUTES).splitlines(), 1):
        for pattern in FORBIDDEN_ROUTE_SOURCE_PATTERNS:
            if pattern.search(line):
                findings.append(
                    Finding(
                        path=relative(ROUTES),
                        line=line_no,
                        message=(
                            "static routes must be discovered from formal syntax output, "
                            "not by reading or parsing source files"
                        ),
                    )
                )
    return findings


def check_static_route_boundary() -> list[Finding]:
    """Return all static route boundary findings.

    Inputs:
    - Static-site README and route implementation.

    Outputs:
    - Finding records for every boundary violation.

    Transformation:
    - Combines required marker checks with forbidden source-parser ownership
      scans.
    """

    return marker_findings() + forbidden_source_parser_findings()


def main() -> int:
    """Run the static route boundary checker.

    Inputs:
    - Repository files addressed by module constants.

    Outputs:
    - Process exit code.

    Transformation:
    - Prints stable diagnostics for findings and a compact success message when
      the static route boundary holds.
    """

    findings = check_static_route_boundary()
    if findings:
        for finding in findings:
            print(finding.render())
        return 1

    print("static route boundary check passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
