#!/usr/bin/env python3
"""Validate checked-in browser packaging capability decisions.

Inputs:
- `docs/web/J0_8_BROWSER_PACKAGING_CAPABILITY.md`.

Outputs:
- Exit status 0 when the decision record contains the required sections and
  named tool boundaries.
- Exit status 1 with stable diagnostics when the browser packaging decision is
  missing or incomplete.

Transformation:
- Reads the decision record as text and verifies the release-critical
  capability terms used by the 0.0.4 roadmap.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
DECISION = ROOT / "docs" / "web" / "J0_8_BROWSER_PACKAGING_CAPABILITY.md"
REQUIRED_TERMS = (
    "# J0.8 Browser Packaging Capability Decision",
    "## Oxc Component Checked",
    "## Oxc Result",
    "## SWC Component Checked",
    "## Rsbuild/Rspack Component Checked",
    "## Rsbuild/Rspack Result",
    "## Selected Implementation",
    "## Release Gate",
    "oxc_parser",
    "oxc_codegen",
    "oxc_resolver",
    "Rsbuild is the preferred user-hidden web build facade",
    "direct Rsbuild configuration is reserved",
    "rspack_core",
    "rspack_plugin_asset",
    "rspack_plugin_html",
    "rspack_plugin_hmr",
    "rspack_watcher",
    "Rust/Tokio",
    "terlc serve",
    "_build/web/",
)


@dataclass(frozen=True)
class Finding:
    """One browser capability decision validation finding.

    Inputs:
    - `message`: human-readable validation failure.

    Outputs:
    - Immutable finding record.

    Transformation:
    - Keeps checker failures structured before rendering them for Make/CI.
    """

    message: str

    def render(self) -> str:
        """Return a stable diagnostic line.

        Inputs:
        - Finding message.

        Outputs:
        - Human-readable diagnostic text.

        Transformation:
        - Prefixes the message with the checked decision path.
        """

        return f"{DECISION.relative_to(ROOT)}: {self.message}"


def check_decision() -> list[Finding]:
    """Validate the browser packaging decision record.

    Inputs:
    - Checked-in decision markdown.

    Outputs:
    - Empty list when the decision is complete enough for J0.8.
    - Finding records for missing files or required terms.

    Transformation:
    - Reads the decision text and checks for the release-critical headings and
      tool names that anchor later browser packaging implementation.
    """

    if not DECISION.is_file():
        return [Finding("missing J0.8 browser packaging capability decision")]

    text = DECISION.read_text(encoding="utf-8")
    findings: list[Finding] = []
    for term in REQUIRED_TERMS:
        if term not in text:
            findings.append(Finding(f"missing required term `{term}`"))
    return findings


def main() -> int:
    """Run the browser packaging capability decision checker.

    Inputs:
    - Current repository docs tree.

    Outputs:
    - Exit status 0 when the decision passes.
    - Exit status 1 with diagnostics when required capability text is missing.

    Transformation:
    - Converts structured findings into stable Make/CI output.
    """

    findings = check_decision()
    if findings:
        print("web-capability-decision-check failed:")
        for finding in findings:
            print(f"  - {finding.render()}")
        return 1
    print("[web-capability-decision] J0.8 browser packaging boundary is documented.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
