#!/usr/bin/env python3
"""Build `.typi` summaries for `std/` modules used by Terlan checks."""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
STD_DIR = ROOT / "std"
OUT_DIR = STD_DIR / "summaries"


def run_emit(source: Path) -> str | None:
    """Emit interface metadata for one stdlib source file.

    Inputs:
    - `source`: absolute path to a `.terl` source under `std/`.

    Outputs:
    - `None` when `terlc emit` succeeds.
    - A diagnostic string when the emit command fails.

    Transformation:
    - Runs `cargo run -p terlan_cli -- emit` with the std summary output
      directory so checked-in std interfaces can be regenerated from source.
    """

    result = subprocess.run(
        [
            "cargo",
            "run",
            "-p",
            "terlan_cli",
            "--",
            "emit",
            str(source.relative_to(ROOT)),
            "--out-dir",
            str(OUT_DIR),
            "--native-policy",
            "safe_native_optional",
        ],
        cwd=ROOT,
        env=os.environ.copy(),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        return f"{source}: emit failed\n{(result.stdout + result.stderr).rstrip()}"
    return None


def main() -> int:
    """Regenerate checked-in stdlib interface summaries.

    Inputs:
    - The repository `std/` tree and local Rust/Cargo toolchain.

    Outputs:
    - Exit status 0 when all selected stdlib sources emit summaries.
    - Exit status 1 when the std tree is missing or any source fails emission.

    Transformation:
    - Scans release stdlib sources, emits `.typi` summaries into
      `std/summaries`, and removes incidental `.erl` files from that directory.
    """

    if not STD_DIR.is_dir():
        print("[build-stdlib-interfaces] std/ directory missing", file=sys.stderr)
        return 1

    OUT_DIR.mkdir(parents=True, exist_ok=True)

    sources = [
        path
        for path in STD_DIR.rglob("*.terl")
        if path.is_file()
        and "summaries" not in path.relative_to(STD_DIR).parts
        and "disabled" not in path.relative_to(STD_DIR).parts
    ]

    failures: list[str] = []
    env = os.environ.copy()
    env.setdefault("CARGO_TERM_COLOR", "never")

    pending = sorted(sources)
    while pending:
        next_pending: list[tuple[Path, str]] = []
        emitted_count = 0

        for source in pending:
            output = run_emit(source)
            if output:
                next_pending.append((source, output))
            else:
                emitted_count += 1

        if not next_pending:
            break

        if emitted_count == 0:
            failures = [output for _source, output in next_pending]
            break

        pending = [source for source, _output in next_pending]

    # Keep summaries tidy: Terlan `emit` writes `.erl` beside `.typi` in the same
    # directory; stdlib interface directories should only need interface metadata.
    for file in OUT_DIR.glob("*.erl"):
        try:
            file.unlink()
        except OSError:
            # A best-effort cleanup; summary generation should still succeed even if
            # cleanup fails for a transient reason.
            pass

    if failures:
        print("[build-stdlib-interfaces] failures:", file=sys.stderr)
        for failure in failures:
            print(f"  - {failure}", file=sys.stderr)
        return 1

    print(f"[build-stdlib-interfaces] wrote {len(sources)} interfaces to {OUT_DIR}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
