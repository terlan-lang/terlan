#!/usr/bin/env python3
"""Stage a local experimental OTP compatibility runtime payload."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path


EXCLUDED_DIR_NAMES = {"src", "doc", "man", "examples"}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--dest", required=True, type=Path)
    args = parser.parse_args()

    source = args.source.resolve()
    dest = args.dest.resolve()
    if not source.is_dir():
        print(
            f"OTP compatibility runtime source does not exist: {source}",
            file=sys.stderr,
        )
        return 1

    shutil.rmtree(dest, ignore_errors=True)
    copy_runtime_tree(source, dest)
    return smoke_runtime(dest)


def copy_runtime_tree(source: Path, dest: Path) -> None:
    def ignore_names(path: str, names: list[str]) -> set[str]:
        ignored = {
            name
            for name in names
            if name in EXCLUDED_DIR_NAMES or name.endswith("_test")
        }
        return ignored

    shutil.copytree(source, dest, ignore=ignore_names, symlinks=True)


def smoke_runtime(runtime: Path) -> int:
    erl = runtime / "bin" / "erl"
    erlc = runtime / "bin" / "erlc"
    if not erl.is_file() or not os.access(erl, os.X_OK):
        print(f"staged OTP compatibility runtime is missing executable {erl}", file=sys.stderr)
        return 1
    if not erlc.is_file() or not os.access(erlc, os.X_OK):
        print(f"staged OTP compatibility runtime is missing executable {erlc}", file=sys.stderr)
        return 1

    release = subprocess.run(
        [
            str(erl),
            "-noshell",
            "-eval",
            'io:format("~s~n", [erlang:system_info(otp_release)]), halt().',
        ],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if release.returncode != 0:
        print(release.stderr, file=sys.stderr, end="")
        return release.returncode
    if release.stdout.strip() != "30":
        print(
            f"staged OTP compatibility runtime expected OTP release 30, got {release.stdout.strip()!r}",
            file=sys.stderr,
        )
        return 1

    smoke_dir = runtime.parent / ".otp-runtime-smoke"
    smoke_dir.mkdir(parents=True, exist_ok=True)
    smoke_source = smoke_dir / "smoke.erl"
    smoke_source.write_text(
        "-module(smoke).\n-export([main/0]).\nmain() -> ok.\n",
        encoding="utf-8",
    )
    compile_result = subprocess.run(
        [str(erlc), "-o", str(smoke_dir), str(smoke_source)],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if compile_result.returncode != 0:
        print(compile_result.stderr, file=sys.stderr, end="")
        return compile_result.returncode
    if not (smoke_dir / "smoke.beam").is_file():
        print("staged OTP compatibility runtime erlc smoke did not emit smoke.beam", file=sys.stderr)
        return 1
    shutil.rmtree(smoke_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
