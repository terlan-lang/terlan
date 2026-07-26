#!/usr/bin/env python3
"""Deterministically identify one Git revision plus its local working tree."""

from __future__ import annotations

import hashlib
import os
import subprocess
from pathlib import Path


DOMAIN = b"terlan.source-tree.v1\0"


def _git(root: Path, *arguments: str) -> bytes:
    """Return exact stdout from one read-only Git command."""

    return subprocess.run(
        ["git", *arguments],
        cwd=root,
        check=True,
        capture_output=True,
    ).stdout


def _frame(digest: "hashlib._Hash", value: bytes) -> None:
    """Add one length-delimited byte sequence to a digest."""

    digest.update(len(value).to_bytes(8, "big"))
    digest.update(value)


def source_revision(root: Path) -> str:
    """Return the full checked-out Git revision."""

    revision = _git(root, "rev-parse", "HEAD").decode("ascii").strip()
    if (
        len(revision) != 40
        or any(character not in "0123456789abcdef" for character in revision)
    ):
        raise AssertionError("checked-out source revision is not a full Git identity")
    return revision


def source_tree_identity(root: Path, revision: str | None = None) -> tuple[bool, str]:
    """Return clean state and a digest covering tracked and untracked content."""

    revision = revision or source_revision(root)
    tracked_diff = _git(
        root,
        "diff",
        "--binary",
        "--no-ext-diff",
        "--no-textconv",
        revision,
        "--",
    )
    untracked_output = _git(
        root,
        "ls-files",
        "--others",
        "--exclude-standard",
        "-z",
    )
    untracked = sorted(path for path in untracked_output.split(b"\0") if path)

    digest = hashlib.sha256()
    digest.update(DOMAIN)
    _frame(digest, revision.encode("ascii"))
    _frame(digest, tracked_diff)
    digest.update(len(untracked).to_bytes(8, "big"))
    for encoded_path in untracked:
        path = root / os.fsdecode(encoded_path)
        _frame(digest, encoded_path)
        if path.is_symlink():
            digest.update(b"L")
            _frame(digest, os.fsencode(os.readlink(path)))
        elif path.is_file():
            digest.update(b"F")
            _frame(digest, path.read_bytes())
        else:
            raise AssertionError(f"unsupported untracked source entry `{path}`")
    return not tracked_diff and not untracked, digest.hexdigest()
