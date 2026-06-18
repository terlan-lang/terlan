#!/usr/bin/env python3
"""Check that CLI Makefile exact-test selectors resolve to real Cargo tests.

Inputs:
- `crates/terlan_cli/cli.mk`, which owns CLI-specific Make targets.
- `cargo test -p terlan_cli -- --list`, which reports the current test names.

Outputs:
- Exit status 0 when every `TERLC_EXACT_TEST` selector in `cli.mk` resolves.
- Exit status 1 with stale selectors when any exact-test selector drifted.

Transformation:
- Extracts exact-test selectors from Make recipes.
- Normalizes Cargo's test-list output into a set of fully qualified test names.
- Compares selectors against that set so test extraction and module renames do
  not silently break formal gates.
"""

from __future__ import annotations

from pathlib import Path
import re
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
CLI_MAKEFILE = ROOT / "crates" / "terlan_cli" / "cli.mk"
SELECTOR_PATTERN = re.compile(r"TERLC_EXACT_TEST\)\s+([^\s]+)\s+--\s+--exact")


def cargo_test_names() -> set[str]:
    """Return fully qualified `terlan_cli` Cargo test names.

    Inputs:
    - The local Cargo workspace and `terlan_cli` crate.

    Outputs:
    - Set of test names as accepted by `cargo test -p terlan_cli <name> -- --exact`.

    Transformation:
    - Runs Cargo's test-list mode and keeps lines ending in `: test`.
    """

    output = subprocess.check_output(
        ["cargo", "test", "-p", "terlan_cli", "--", "--list"],
        cwd=ROOT,
        text=True,
    )
    return {
        line.split(": test", 1)[0]
        for line in output.splitlines()
        if ": test" in line
    }


def exact_selectors() -> list[str]:
    """Return exact-test selectors referenced by `cli.mk`.

    Inputs:
    - `crates/terlan_cli/cli.mk`.

    Outputs:
    - Ordered list of selector strings.

    Transformation:
    - Applies the Make recipe selector regex without interpreting Make
      variables or shell commands.
    """

    text = CLI_MAKEFILE.read_text(encoding="utf-8")
    return SELECTOR_PATTERN.findall(text)


def stale_selectors(selectors: list[str], tests: set[str]) -> list[str]:
    """Return selectors that do not resolve to current Cargo tests.

    Inputs:
    - `selectors`: exact-test selectors from `cli.mk`.
    - `tests`: fully qualified test names from Cargo.

    Outputs:
    - Ordered list of stale selectors.

    Transformation:
    - Filters selectors not present in Cargo's current test-name set.
    """

    return [selector for selector in selectors if selector not in tests]


def main() -> int:
    """Run the CLI exact-selector check.

    Inputs:
    - Current repository checkout.

    Outputs:
    - Process exit code for Make/CI integration.

    Transformation:
    - Composes selector extraction, Cargo test discovery, and stale-selector
      reporting into a single release gate.
    """

    selectors = exact_selectors()
    missing = stale_selectors(selectors, cargo_test_names())
    if missing:
        print("[cli-exact-selector] stale exact test selectors:")
        for selector in missing:
            print(selector)
        return 1
    print(f"[cli-exact-selector] {len(selectors)} exact selectors resolve.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
