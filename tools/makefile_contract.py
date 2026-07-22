"""Shared parsing helpers for repository Make contract validators."""

from __future__ import annotations

from pathlib import Path
import re
from typing import Iterable


TARGET_PATTERN = re.compile(r"^([A-Za-z0-9_.-]+):(?:\s|$)")


def make_targets_from_text(makefile_text: str) -> set[str]:
    """Return concrete single-name targets declared in Makefile text."""
    return {
        match.group(1)
        for line in makefile_text.splitlines()
        if (match := TARGET_PATTERN.match(line)) is not None
    }


def make_targets_from_paths(paths: Iterable[Path]) -> set[str]:
    """Return the union of targets declared by the supplied Makefiles."""
    targets: set[str] = set()
    for path in paths:
        targets.update(make_targets_from_text(path.read_text(encoding="utf-8")))
    return targets


def make_target_body(makefile_text: str, target: str) -> list[str] | None:
    """Return normalized command lines owned by one concrete Make target."""
    lines = makefile_text.splitlines()
    for index, line in enumerate(lines):
        match = TARGET_PATTERN.match(line)
        if match is None or match.group(1) != target:
            continue
        body: list[str] = []
        for candidate in lines[index + 1 :]:
            if candidate.startswith("\t"):
                body.append(candidate.removeprefix("\t").strip())
                continue
            if TARGET_PATTERN.match(candidate):
                break
        return body
    return None


def make_target_prerequisites(makefile_text: str, target: str) -> list[str] | None:
    """Return prerequisite names declared by one concrete Make target."""
    lines = makefile_text.splitlines()
    for index, line in enumerate(lines):
        match = TARGET_PATTERN.match(line)
        if match is None or match.group(1) != target:
            continue
        declaration = line.split(":", 1)[1].strip()
        prerequisites: list[str] = []
        while True:
            continued = declaration.endswith("\\")
            if continued:
                declaration = declaration[:-1].rstrip()
            prerequisites.extend(declaration.split())
            if not continued:
                return prerequisites
            index += 1
            if index >= len(lines):
                return prerequisites
            declaration = lines[index].strip()
    return None
