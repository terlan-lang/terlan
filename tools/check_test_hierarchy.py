#!/usr/bin/env python3
"""Check that Makefile script gates stay in the allowed test hierarchy.

Inputs:
- Public Makefiles that route repository checks.
- Script paths invoked from those Makefiles.

Outputs:
- Exit status 0 when script gates are release-owned policy, drift, generator,
  or cross-command orchestration checks.
- Exit status 1 with path-specific diagnostics for missing, crate-local, or
  behavioral script-only gates.

Transformation:
- Parses simple Make recipe lines that invoke `bash`, `sh`, `python`, or
  `python3`.
- Resolves the referenced script path relative to the repository root.
- Classifies script paths against the allowed 0.0.4 test hierarchy contract.
"""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import shlex


ROOT = Path(__file__).resolve().parents[1]
MAKEFILES = [
    ROOT / "Makefile",
    ROOT / "crates" / "terlan_cli" / "cli.mk",
    ROOT / "std" / "stdlib.mk",
]
SCRIPT_COMMANDS = {"bash", "sh", "python", "python3"}
ALLOWED_EXACT = {
    Path("tools/validate_ebnf.py"),
    Path("std/scripts/build_interfaces.py"),
    Path("std/scripts/run_release_tests.sh"),
}
ALLOWED_PREFIXES = (
    Path("tools"),
    Path("scripts"),
    Path("std/scripts"),
)


@dataclass(frozen=True)
class ScriptInvocation:
    """Script command referenced by a public Makefile recipe.

    Inputs:
    - `makefile`: repository-relative Makefile path.
    - `line_no`: 1-based recipe line number.
    - `script`: repository-relative script path.

    Outputs:
    - Immutable invocation record for diagnostics and classification.

    Transformation:
    - Stores only the command shape needed by the hierarchy checker.
    """

    makefile: Path
    line_no: int
    script: Path

    def diagnostic_prefix(self) -> str:
        """Return the stable diagnostic prefix for this invocation.

        Inputs:
        - The invocation source location.

        Outputs:
        - `path:line` text used in failure messages.

        Transformation:
        - Joins repository-relative Makefile path and line number.
        """

        return f"{self.makefile}:{self.line_no}"


def normalize_recipe_line(line: str) -> str:
    """Return a shell-like recipe line without Make command prefixes.

    Inputs:
    - Raw Makefile line.

    Outputs:
    - Stripped command text.

    Transformation:
    - Removes leading whitespace and common Make recipe prefixes such as `@`
      and `-`.
    """

    command = line.strip()
    while command.startswith(("@", "-")):
        command = command[1:].lstrip()
    return command


def script_from_command(command: str) -> Path | None:
    """Extract the first script operand from a shell command.

    Inputs:
    - Normalized Make recipe command.

    Outputs:
    - Repository-relative script path when the command directly invokes a
      script through a known interpreter.
    - `None` for non-script commands.

    Transformation:
    - Uses shell tokenization for simple recipe lines and ignores inline shell
      snippets that do not pass a `.py` or `.sh` path to the interpreter.
    """

    try:
        parts = shlex.split(command, comments=False, posix=True)
    except ValueError:
        return None
    if len(parts) < 2 or parts[0] not in SCRIPT_COMMANDS:
        return None
    for part in parts[1:]:
        if part.startswith("-"):
            continue
        path = Path(part)
        if path.suffix in {".py", ".sh"}:
            return path
    return None


def iter_script_invocations() -> list[ScriptInvocation]:
    """Return script invocations from public Makefiles.

    Inputs:
    - Root Makefile and included module Makefiles.

    Outputs:
    - Sorted script invocation records.

    Transformation:
    - Reads Makefiles line by line and extracts direct interpreter-script
      invocations from recipe lines.
    """

    invocations: list[ScriptInvocation] = []
    for makefile in MAKEFILES:
        relative = makefile.relative_to(ROOT)
        for line_no, line in enumerate(makefile.read_text(encoding="utf-8").splitlines(), 1):
            script = script_from_command(normalize_recipe_line(line))
            if script is not None:
                invocations.append(ScriptInvocation(relative, line_no, script))
    return invocations


def is_allowed_script(script: Path) -> bool:
    """Return whether a script path fits the allowed hierarchy.

    Inputs:
    - Repository-relative script path.

    Outputs:
    - `True` for policy, drift, generator, and orchestration scripts.
    - `False` for crate-local or feature-behavior scripts.

    Transformation:
    - Allows exact generator/orchestration scripts.
    - Allows `check_*` scripts under release-owned script roots.
    """

    if script in ALLOWED_EXACT:
        return True
    if script.name.startswith("check_") and any(script.is_relative_to(prefix) for prefix in ALLOWED_PREFIXES):
        return True
    return False


def check_invocation(invocation: ScriptInvocation) -> list[str]:
    """Validate one script invocation.

    Inputs:
    - Script invocation discovered from a Makefile.

    Outputs:
    - Empty list when the invocation satisfies Q0.5.
    - Diagnostics explaining missing or disallowed script usage.

    Transformation:
    - Resolves the script path relative to the repository root and checks
      existence plus hierarchy classification.
    """

    diagnostics: list[str] = []
    prefix = invocation.diagnostic_prefix()
    absolute = ROOT / invocation.script
    if not absolute.exists():
        diagnostics.append(f"{prefix}: script `{invocation.script}` does not exist")
    if invocation.script.parts[:2] == ("crates", "terlan_cli"):
        diagnostics.append(
            f"{prefix}: script `{invocation.script}` is crate-local; move policy scripts to scripts/ or std/scripts/"
        )
    if not is_allowed_script(invocation.script):
        diagnostics.append(
            f"{prefix}: script `{invocation.script}` is not an allowed policy, drift, generator, or orchestration gate"
        )
    return diagnostics


def main() -> int:
    """Run the test hierarchy check.

    Inputs:
    - Public Makefile script invocations.

    Outputs:
    - Exit status 0 when all script gates are allowed.
    - Exit status 1 with diagnostics when a script gate violates Q0.5.

    Transformation:
    - Aggregates invocation diagnostics and prints a stable summary.
    """

    diagnostics: list[str] = []
    invocations = iter_script_invocations()
    for invocation in invocations:
        diagnostics.extend(check_invocation(invocation))

    if diagnostics:
        print("[test-hierarchy] failures:")
        for diagnostic in diagnostics:
            print(f"  - {diagnostic}")
        return 1

    print(f"[test-hierarchy] {len(invocations)} Makefile script gates are release-owned.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
