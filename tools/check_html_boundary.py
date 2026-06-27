#!/usr/bin/env python3
"""Check that generated HTML mutation and escaping stay in the HTML feature.

Inputs:
- `crates/terlan/src/html/README.md`.
- `crates/terlan/src/html/escaping.rs`.
- CLI static-site and documentation renderer source files.

Outputs:
- Exit status 0 when generated HTML base-path injection and escaping are
  routed through `terlan_html`.
- Exit status 1 with stable diagnostics when command modules grow local HTML
  mutation or entity-escaping implementation.

Transformation:
- Requires the shared HTML feature to document and expose the base-path and
  escaping boundaries.
- Requires static-site and documentation renderers to call the shared helpers.
- Rejects obvious inline `<base href>` construction and raw HTML entity
  replacement chains in CLI production renderer code.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
HTML_ROOT = ROOT / "crates" / "terlan" / "src" / "html"
HTML_README = HTML_ROOT / "README.md"
HTML_ESCAPING = HTML_ROOT / "escaping.rs"
STATIC_SITE_ROOT = ROOT / "crates" / "terlan" / "src" / "commands" / "static_site"
STATIC_SITE_MOD = STATIC_SITE_ROOT / "mod.rs"
STATIC_SITE_RENDER = STATIC_SITE_ROOT / "render.rs"
DOC_COMMAND = ROOT / "crates" / "terlan" / "src" / "commands" / "doc" / "mod.rs"
DOC_RENDER = ROOT / "crates" / "terlan" / "src" / "commands" / "doc" / "render.rs"

REQUIRED_BOUNDARY_MARKERS = (
    (
        HTML_README,
        "Inject static-site `<base href>` metadata through the shared HTML boundary.",
    ),
    (
        HTML_README,
        "Escape generated HTML text nodes and attribute values through the shared HTML",
    ),
    (
        HTML_ESCAPING,
        "ammonia::clean_text(text)",
    ),
    (
        STATIC_SITE_MOD,
        "crate::terlan_html::inject_html_base_path",
    ),
    (
        STATIC_SITE_RENDER,
        "use crate::terlan_html::{escape_html_attr, escape_html_text};",
    ),
    (
        DOC_COMMAND,
        "use crate::terlan_html::escape_html_text;",
    ),
    (
        DOC_RENDER,
        "use crate::terlan_html::escape_html_text;",
    ),
)

FORBIDDEN_CLI_HTML_PATTERNS = (
    re.compile(r"""format!\s*\(\s*r?#?["']<base\s+href="""),
    re.compile(r"""\.replace\s*\(\s*['"]&['"]\s*,\s*['"]&amp;['"]\s*\)"""),
    re.compile(r"""\.replace\s*\(\s*['"]<['"]\s*,\s*['"]&lt;['"]\s*\)"""),
    re.compile(r"""\.replace\s*\(\s*['"]>['"]\s*,\s*['"]&gt;['"]\s*\)"""),
)


@dataclass(frozen=True)
class Finding:
    """HTML boundary finding.

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


def cli_renderer_files() -> list[Path]:
    """Return production CLI renderer files that may emit HTML.

    Inputs:
    - Static-site and documentation command directories.

    Outputs:
    - Sorted Rust implementation paths excluding tests.

    Transformation:
    - Keeps tests out of forbidden-pattern scanning because tests may contain
      literal HTML expected output while production code must use `terlan_html`.
    """

    static_files = [
        path
        for path in STATIC_SITE_ROOT.glob("*.rs")
        if not path.name.endswith("_test.rs")
    ]
    doc_files = [DOC_COMMAND, DOC_RENDER]
    return sorted(static_files + doc_files)


def marker_findings() -> list[Finding]:
    """Return missing shared HTML boundary marker findings.

    Inputs:
    - Files and marker text from `REQUIRED_BOUNDARY_MARKERS`.

    Outputs:
    - Finding records for missing documentation or call-site markers.

    Transformation:
    - Scans exact marker text so the checker locks the current shared
      `terlan_html` ownership boundary.
    """

    findings: list[Finding] = []
    for path, marker in REQUIRED_BOUNDARY_MARKERS:
        if marker not in read_text(path):
            findings.append(
                Finding(
                    path=relative(path),
                    line=None,
                    message=f"missing shared HTML boundary marker `{marker}`",
                )
            )
    return findings


def forbidden_cli_html_findings() -> list[Finding]:
    """Return local CLI HTML mutation or escaping findings.

    Inputs:
    - Production static-site and documentation renderer files.

    Outputs:
    - Finding records for local base-tag or entity-escaping implementation.

    Transformation:
    - Searches line by line so regressions point at the exact source location.
    """

    findings: list[Finding] = []
    for path in cli_renderer_files():
        for line_no, line in enumerate(read_text(path).splitlines(), 1):
            for pattern in FORBIDDEN_CLI_HTML_PATTERNS:
                if pattern.search(line):
                    findings.append(
                        Finding(
                            path=relative(path),
                            line=line_no,
                            message="generated HTML mutation and escaping must stay behind terlan_html",
                        )
                    )
    return findings


def check_html_boundary() -> list[Finding]:
    """Return all shared HTML boundary findings.

    Inputs:
    - Shared HTML crate docs/helpers and CLI renderer files.

    Outputs:
    - Finding records for every shared HTML boundary violation.

    Transformation:
    - Combines required shared-boundary marker checks with local CLI renderer
      implementation scans.
    """

    return marker_findings() + forbidden_cli_html_findings()


def main() -> int:
    """Run the shared HTML boundary checker.

    Inputs:
    - Repository files addressed by module constants.

    Outputs:
    - Process exit code.

    Transformation:
    - Prints stable diagnostics for findings and a compact success message when
      the generated HTML boundary holds.
    """

    findings = check_html_boundary()
    if findings:
        for finding in findings:
            print(finding.render())
        return 1
    print("HTML boundary OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
