#!/usr/bin/env python3
"""Reject retired runtime payloads from repository and release trees."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


FORBIDDEN_FILE_ENDINGS = (".tvm.json", ".tvm.reuse", ".vmir", ".coreir")
SERIALIZED_VMIR_MARKERS = (b'"vm_ir"', b'"instructions"')


def transition_payload_violations(root: Path) -> list[str]:
    """Return deterministic retired-runtime violations below one tree."""

    violations: list[str] = []
    for path in sorted(candidate for candidate in root.rglob("*") if candidate.is_file()):
        relative = path.relative_to(root).as_posix()
        lowered = path.name.lower()
        if lowered.endswith(FORBIDDEN_FILE_ENDINGS):
            violations.append(f"{relative}: retired runtime artifact filename")
            continue

        if path.suffix.lower() not in {".json", ".tvm"}:
            continue
        payload = path.read_bytes()
        if path.suffix.lower() == ".tvm" and payload.lstrip().startswith((b"{", b"[")):
            violations.append(f"{relative}: JSON payload renamed as native image")
            continue
        if all(marker in payload for marker in SERIALIZED_VMIR_MARKERS):
            violations.append(f"{relative}: serialized VMIR instruction payload")
    return violations


def assert_no_transition_payloads(root: Path) -> None:
    """Require one tree to contain no retired executable runtime payloads."""

    violations = transition_payload_violations(root)
    if violations:
        rendered = "\n".join(f"- {violation}" for violation in violations)
        raise AssertionError(
            f"retired runtime payloads remain below `{root}`:\n{rendered}"
        )


def parse_args(argv: list[str]) -> argparse.Namespace:
    """Parse one or more repository or release roots to inspect."""

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("roots", nargs="+", type=Path)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    """Scan requested trees and emit one stable gate result."""

    args = parse_args(sys.argv[1:] if argv is None else argv)
    try:
        for root in args.roots:
            if not root.is_dir():
                raise FileNotFoundError(f"transition scan root is missing: {root}")
            assert_no_transition_payloads(root)
    except (AssertionError, OSError) as error:
        print(f"release transition scan failed: {error}", file=sys.stderr)
        return 1
    print(f"release transition scan passed for {len(args.roots)} tree(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
