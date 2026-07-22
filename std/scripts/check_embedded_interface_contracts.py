#!/usr/bin/env python3
"""Verify release-critical embedded std interface summaries are fresh."""

from __future__ import annotations

import difflib
import tempfile
from pathlib import Path

from build_interfaces import OUT_DIR, ROOT, ensure_compiler, run_emit


SOURCES = (
    ROOT / "std/core/Option.terl",
    ROOT / "std/collections/KeyedEnumerable.terl",
    ROOT / "std/vm/DistributedStorage.terl",
)


def artifact_names(source: Path) -> tuple[str, str]:
    """Return deterministic interface artifact names for one std source."""

    module_name = "std." + ".".join(source.relative_to(ROOT / "std").with_suffix("").parts)
    return f"{module_name}.typi", f"{module_name}.typi.deps"


def artifact_diff(expected: Path, generated: Path) -> str | None:
    """Return a unified diff when one generated artifact is stale or missing."""

    if not expected.is_file():
        return f"missing committed artifact: {expected.relative_to(ROOT)}"
    if not generated.is_file():
        return f"generator did not emit: {generated.name}"
    expected_text = expected.read_text(encoding="utf-8")
    generated_text = generated.read_text(encoding="utf-8")
    if expected_text == generated_text:
        return None
    return "".join(
        difflib.unified_diff(
            expected_text.splitlines(keepends=True),
            generated_text.splitlines(keepends=True),
            fromfile=str(expected.relative_to(ROOT)),
            tofile=f"generated/{generated.name}",
        )
    )


def main() -> int:
    """Regenerate and compare embedded interface contracts in temporary storage."""

    compiler_failure = ensure_compiler()
    if compiler_failure is not None:
        print(f"embedded interface compiler build failed:\n{compiler_failure}")
        return 1

    failures: list[str] = []
    with tempfile.TemporaryDirectory(prefix="terlan-embedded-interfaces-") as temp:
        generated_dir = Path(temp)
        for source in SOURCES:
            failure = run_emit(source, generated_dir)
            if failure is not None:
                failures.append(failure)
                continue
            for name in artifact_names(source):
                diff = artifact_diff(OUT_DIR / name, generated_dir / name)
                if diff is not None:
                    failures.append(diff)

    if failures:
        print("embedded std interface contract check failed:")
        for failure in failures:
            print(failure.rstrip())
        return 1

    print("embedded std interface contracts are canonical and fresh.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
