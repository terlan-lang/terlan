#!/usr/bin/env python3
"""Validate the retired AOT mini-roadmap's main-roadmap closeout record."""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

import check_tvm_aot_platform_matrix as platform_matrix
import check_tvm_aot_release_closeout as release_closeout


ROOT = Path(__file__).resolve().parents[1]
MAIN_ROADMAP = ROOT / "docs/roadmap/ROADMAP_0_0_7.md"
REPORT = ROOT / "target/quality/tvm-aot-release-closeout-report.json"
CHECKBOX = re.compile(r"^- \[([ xX])\] (Slice 100|Slice 101[A-F])(?::| )")
REQUIRED_SLICES = (
    "Slice 100",
    "Slice 101A",
    "Slice 101B",
    "Slice 101C",
    "Slice 101D",
    "Slice 101E",
    "Slice 101F",
)


def checklist(markdown: str) -> dict[str, bool]:
    """Parse the unique main-roadmap AOT ownership rows."""

    items: dict[str, bool] = {}
    for line in markdown.splitlines():
        match = CHECKBOX.match(line)
        if match is None:
            continue
        identity = match.group(2)
        if identity in items:
            raise AssertionError(f"main roadmap duplicates `{identity}`")
        items[identity] = match.group(1).lower() == "x"
    missing = sorted(set(REQUIRED_SLICES) - set(items))
    if missing:
        raise AssertionError("main roadmap omits AOT slices: " + ", ".join(missing))
    return items


def validate_report(report: object, revision: str) -> None:
    """Validate the repository-local AOT closeout report."""

    if not isinstance(report, dict):
        raise AssertionError("AOT closeout report is not an object")
    if report.get("schema") != release_closeout.SCHEMA:
        raise AssertionError("AOT closeout report has an unexpected schema")
    if report.get("decision") != "pass":
        raise AssertionError("AOT closeout report did not pass")
    if report.get("source_revision") != revision:
        raise AssertionError("AOT closeout report belongs to another revision")
    if report.get("local_gates") != list(release_closeout.LOCAL_GATES):
        raise AssertionError("AOT closeout report omits local gates")
    if report.get("publishing_required") is not False:
        raise AssertionError("AOT closeout incorrectly requires publishing")
    if report.get("semantic_preservation") != {
        "runtime_fallbacks": 0,
        "temporary_migration_support": 0,
        "deletion_debt": 0,
    }:
        raise AssertionError("AOT closeout retains migration debt")
    contract = report.get("platform_contract")
    if not isinstance(contract, dict):
        raise AssertionError("AOT closeout omits the platform contract")
    if contract.get("supported_targets") != list(platform_matrix.TARGETS):
        raise AssertionError("AOT closeout has an incomplete platform contract")
    host = contract.get("host_report")
    if not isinstance(host, dict) or not platform_matrix.is_sha256(host.get("sha256")):
        raise AssertionError("AOT closeout omits executable host evidence")


def check(root: Path = ROOT) -> None:
    """Require all main AOT slices and one matching local closeout report."""

    markdown = (root / MAIN_ROADMAP.relative_to(ROOT)).read_text(encoding="utf-8")
    items = checklist(markdown)
    unchecked = [item for item in REQUIRED_SLICES if not items[item]]
    if unchecked:
        raise AssertionError("main AOT reconciliation is incomplete: " + ", ".join(unchecked))
    report_path = root / REPORT.relative_to(ROOT)
    try:
        report = json.loads(report_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise AssertionError(f"cannot load local AOT closeout report: {error}") from error
    revision = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout.strip()
    validate_report(report, revision)
    print("TVM AOT roadmap reconciliation passed; mini-roadmap retired")


def self_test() -> None:
    """Prove missing and unchecked main AOT ownership fails closed."""

    valid = "\n".join(f"- [x] {item}: evidence" for item in REQUIRED_SLICES)
    assert checklist(valid) == {item: True for item in REQUIRED_SLICES}
    unchecked = valid.replace("- [x] Slice 101F", "- [ ] Slice 101F")
    assert checklist(unchecked)["Slice 101F"] is False
    for invalid in (
        valid.replace("- [x] Slice 101F: evidence", ""),
        valid + "\n- [x] Slice 100: duplicate",
    ):
        try:
            checklist(invalid)
        except AssertionError:
            pass
        else:
            raise AssertionError("reconciliation accepted malformed AOT ownership")
    print("TVM AOT roadmap reconciliation self-test passed")


def main() -> int:
    """Dispatch reconciliation or its contract self-test."""

    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("check", "self-test"))
    command = parser.parse_args().command
    try:
        if command == "check":
            check()
        else:
            self_test()
    except (AssertionError, OSError, subprocess.CalledProcessError) as error:
        print(f"TVM AOT roadmap reconciliation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
