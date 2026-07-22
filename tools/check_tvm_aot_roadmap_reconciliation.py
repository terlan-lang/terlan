#!/usr/bin/env python3
"""Validate evidence-backed reconciliation of the AOT roadmaps."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

import check_tvm_aot_release_closeout as release_closeout


ROOT = Path(__file__).resolve().parents[1]
MAIN_ROADMAP = "ROADMAP_0_0_7.md"
AOT_ROADMAP = "ROADMAP_0_0_7_AOT.md"
ATTESTATION = ROOT / "docs/release/evidence/0.0.7-aot-closeout.json"
CHECKBOX = re.compile(r"^\s*- \[([ xX])\] (.+?)(?::|$)")
REVISION = re.compile(r"^[0-9a-f]{40}$")
MAIN_OWNERS = {
    "Slice 100": ("AOT-1", "AOT-2", "AOT-3", "AOT-4"),
    "Slice 101A": ("AOT-8",),
    "Slice 101B": ("AOT-6D1",),
    "Slice 101C": ("AOT-6C3",),
    "Slice 101D": ("AOT-4",),
    "Slice 101E": ("AOT-7",),
    "Slice 101F": ("AOT-5", "AOT-6", "AOT-8"),
}


def find_roadmap(name: str, root: Path = ROOT) -> Path:
    """Find one active roadmap in the repository or documentation workspace."""

    candidates = (root / "docs/roadmap" / name, root.parent / "docs/roadmap" / name)
    for candidate in candidates:
        if candidate.is_file():
            return candidate
    rendered = " or ".join(f"`{candidate}`" for candidate in candidates)
    raise AssertionError(f"missing active roadmap at {rendered}")


def checklist(markdown: str, prefixes: tuple[str, ...]) -> dict[str, bool]:
    """Parse unique top-level checklist identities matching known prefixes."""

    items: dict[str, bool] = {}
    for line in markdown.splitlines():
        match = CHECKBOX.match(line)
        if match is None:
            continue
        title = match.group(2)
        identity = next(
            (prefix for prefix in prefixes if title == prefix or title.startswith(f"{prefix} ")),
            None,
        )
        if identity is None:
            continue
        if identity in items:
            raise AssertionError(f"roadmap contains duplicate checklist item `{identity}`")
        items[identity] = match.group(1).lower() == "x"
    missing = sorted(set(prefixes) - set(items))
    if missing:
        raise AssertionError(f"roadmap omits checklist items: {', '.join(missing)}")
    return items


def validate_closeout_attestation(
    value: object,
    allowed_revisions: set[str],
) -> None:
    """Validate the retained release record needed to close AOT-9."""

    if not isinstance(value, dict):
        raise AssertionError("AOT closeout attestation is not a JSON object")
    if value.get("schema") != release_closeout.SCHEMA:
        raise AssertionError("AOT closeout attestation has an unexpected schema")
    if value.get("decision") != "pass":
        raise AssertionError("AOT closeout attestation did not pass")
    revision = value.get("source_revision")
    if not isinstance(revision, str) or REVISION.fullmatch(revision) is None:
        raise AssertionError("AOT closeout attestation has an invalid source revision")
    if revision not in allowed_revisions:
        raise AssertionError("AOT closeout attestation belongs to an unrelated revision")
    gates = value.get("local_gates")
    if gates != list(release_closeout.LOCAL_GATES):
        raise AssertionError("AOT closeout attestation omits or reorders local gates")
    preservation = value.get("semantic_preservation")
    if preservation != {
        "runtime_fallbacks": 0,
        "temporary_migration_support": 0,
        "deletion_debt": 0,
    }:
        raise AssertionError("AOT closeout attestation retains migration debt")
    for field in ("clean_checkout", "platform_matrix", "thread_sanitizer"):
        evidence = value.get(field)
        if not isinstance(evidence, dict):
            raise AssertionError(f"AOT closeout attestation omits `{field}` evidence")
        if not isinstance(evidence.get("path"), str) or not evidence["path"]:
            raise AssertionError(f"AOT closeout `{field}` evidence omits its path")
        if not release_closeout.platform_matrix.is_sha256(evidence.get("sha256")):
            raise AssertionError(f"AOT closeout `{field}` evidence has an invalid digest")
    retained = value.get("retained_evidence")
    required_retained = set(release_closeout.EVIDENCE) | {"inventory"}
    if not isinstance(retained, dict) or set(retained) != required_retained:
        raise AssertionError("AOT closeout attestation has incomplete retained evidence")
    for name, evidence in retained.items():
        if not isinstance(evidence, dict) or not release_closeout.platform_matrix.is_sha256(
            evidence.get("sha256")
        ):
            raise AssertionError(f"AOT closeout retained evidence `{name}` has an invalid digest")
    inventory = retained["inventory"]
    counts = inventory.get("classification_counts")
    if not isinstance(counts, dict):
        raise AssertionError("AOT closeout attestation omits inventory classifications")
    for classification in ("temporary-migration-support", "deletion-debt"):
        if counts.get(classification) != 0:
            raise AssertionError(f"AOT closeout inventory retains `{classification}`")
    artifact_evidence = value.get("artifact_evidence")
    targets = artifact_evidence.get("targets") if isinstance(artifact_evidence, dict) else None
    if not isinstance(targets, list) or len(targets) != 6:
        raise AssertionError("AOT closeout attestation lacks six-target artifact evidence")
    expected_targets = set(release_closeout.platform_matrix.TARGETS)
    actual_targets: set[str] = set()
    for target in targets:
        if not isinstance(target, dict):
            raise AssertionError("AOT closeout attestation has malformed target evidence")
        target_id = target.get("target_id")
        if not isinstance(target_id, str) or target_id in actual_targets:
            raise AssertionError("AOT closeout attestation has duplicate target evidence")
        actual_targets.add(target_id)
        for field in ("descriptor_digest", "image_sha256"):
            if not release_closeout.platform_matrix.is_sha256(target.get(field)):
                raise AssertionError(
                    f"AOT closeout target `{target_id}` has invalid `{field}`"
                )
    if actual_targets != expected_targets:
        raise AssertionError("AOT closeout attestation has an incomplete target set")


def validate_reconciliation(
    main_markdown: str,
    aot_markdown: str,
    attestation: object | None = None,
    allowed_revisions: set[str] | None = None,
) -> list[str]:
    """Reject main-roadmap checkoffs that precede their AOT evidence owners."""

    owner_names = tuple(sorted({owner for owners in MAIN_OWNERS.values() for owner in owners}))
    main = checklist(main_markdown, tuple(MAIN_OWNERS))
    aot = checklist(aot_markdown, owner_names + ("AOT-9",))
    pending: list[str] = []
    for item, owners in MAIN_OWNERS.items():
        complete = all(aot[owner] for owner in owners)
        if main[item] and not complete:
            missing = ", ".join(owner for owner in owners if not aot[owner])
            raise AssertionError(f"`{item}` is checked before its AOT owners: {missing}")
        if complete and not main[item]:
            pending.append(item)

    if aot["AOT-9"]:
        unchecked = [item for item, complete in main.items() if not complete]
        if unchecked:
            raise AssertionError(
                "AOT-9 is checked before main-roadmap reconciliation: "
                + ", ".join(unchecked)
            )
        if attestation is None:
            raise AssertionError("AOT-9 is checked without a retained closeout attestation")
        validate_closeout_attestation(attestation, allowed_revisions or set())
    return pending


def repository_revisions(root: Path = ROOT) -> set[str]:
    """Return revisions reachable from the current compiler checkout."""

    output = subprocess.run(
        ["git", "rev-list", "HEAD"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return set(output.splitlines())


def record_attestation(
    report_path: Path,
    output_path: Path = ATTESTATION,
    root: Path = ROOT,
) -> Path:
    """Validate and atomically retain one official closeout report."""

    try:
        report = json.loads(report_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise AssertionError(f"cannot load AOT closeout report `{report_path}`: {error}") from error
    validate_closeout_attestation(report, repository_revisions(root))
    output_path.parent.mkdir(parents=True, exist_ok=True)
    payload = json.dumps(report, indent=2, sort_keys=True) + "\n"
    descriptor, temporary_name = tempfile.mkstemp(
        prefix=f".{output_path.name}.",
        dir=output_path.parent,
        text=True,
    )
    temporary = Path(temporary_name)
    try:
        with os.fdopen(descriptor, "w", encoding="utf-8") as target:
            target.write(payload)
            target.flush()
            os.fsync(target.fileno())
        temporary.replace(output_path)
    finally:
        temporary.unlink(missing_ok=True)
    print(f"TVM AOT closeout attestation retained: {output_path}")
    return output_path


def check(root: Path = ROOT) -> None:
    """Validate the live roadmaps and any required retained attestation."""

    main = find_roadmap(MAIN_ROADMAP, root).read_text(encoding="utf-8")
    aot = find_roadmap(AOT_ROADMAP, root).read_text(encoding="utf-8")
    attestation = None
    if ATTESTATION.is_file():
        attestation = json.loads(ATTESTATION.read_text(encoding="utf-8"))
    pending = validate_reconciliation(main, aot, attestation, repository_revisions(root))
    suffix = "none" if not pending else ", ".join(pending)
    print(f"TVM AOT roadmap reconciliation passed; pending checkoffs: {suffix}")


def self_test() -> None:
    """Prove missing owners, premature checkoffs, and stale evidence fail closed."""

    main = "\n".join(f"- [x] {item}: requirement" for item in MAIN_OWNERS)
    owner_names = sorted({owner for owners in MAIN_OWNERS.values() for owner in owners})
    aot = "\n".join(f"- [x] {owner}: requirement" for owner in owner_names)
    aot += "\n- [ ] AOT-9: closeout\n"
    assert validate_reconciliation(main, aot) == []

    early_main = main.replace("- [x] Slice 101F", "- [ ] Slice 101F")
    early_aot = aot.replace("- [x] AOT-6", "- [ ] AOT-6", 1)
    assert validate_reconciliation(early_main, early_aot) == []
    assert validate_reconciliation(early_main, aot) == ["Slice 101F"]
    try:
        validate_reconciliation(main, early_aot)
    except AssertionError as error:
        assert "Slice 101F" in str(error)
    else:
        raise AssertionError("reconciliation accepted an early Slice 101F checkoff")

    try:
        checklist(main + "\n- [x] Slice 100: duplicate", tuple(MAIN_OWNERS))
    except AssertionError:
        pass
    else:
        raise AssertionError("reconciliation accepted a duplicate main slice")

    closed_aot = aot.replace("- [ ] AOT-9", "- [x] AOT-9")
    try:
        validate_reconciliation(early_main, closed_aot)
    except AssertionError as error:
        assert "Slice 101F" in str(error)
    else:
        raise AssertionError("reconciliation accepted AOT-9 before main reconciliation")
    try:
        validate_reconciliation(main, closed_aot)
    except AssertionError as error:
        assert "attestation" in str(error)
    else:
        raise AssertionError("reconciliation accepted AOT-9 without attestation")

    revision = "a" * 40
    report = {
        "schema": release_closeout.SCHEMA,
        "decision": "pass",
        "source_revision": revision,
        "local_gates": list(release_closeout.LOCAL_GATES),
        "semantic_preservation": {
            "runtime_fallbacks": 0,
            "temporary_migration_support": 0,
            "deletion_debt": 0,
        },
        "clean_checkout": {"path": "clean.json", "sha256": "d" * 64},
        "platform_matrix": {"path": "matrix.json", "sha256": "e" * 64},
        "thread_sanitizer": {"path": "sanitizer.json", "sha256": "f" * 64},
        "retained_evidence": {
            name: {
                "path": f"{name}.json",
                "sha256": "1" * 64,
                **(
                    {
                        "classification_counts": {
                            "reusable-runtime-semantics": 1,
                            "compiler-internal-ir": 1,
                            "temporary-migration-support": 0,
                            "deletion-debt": 0,
                        }
                    }
                    if name == "inventory"
                    else {}
                ),
            }
            for name in (*release_closeout.EVIDENCE, "inventory")
        },
        "artifact_evidence": {
            "targets": [
                {
                    "target_id": target_id,
                    "descriptor_digest": "b" * 64,
                    "image_sha256": "c" * 64,
                }
                for target_id in release_closeout.platform_matrix.TARGETS
            ]
        },
    }
    validate_reconciliation(main, closed_aot, report, {revision})
    for mutation in (
        {**report, "source_revision": "b" * 40},
        {**report, "decision": "partial"},
        {**report, "artifact_evidence": {"targets": [{}]}},
    ):
        try:
            validate_reconciliation(main, closed_aot, mutation, {revision})
        except AssertionError:
            pass
        else:
            raise AssertionError("reconciliation accepted invalid closeout evidence")

    with tempfile.TemporaryDirectory(prefix="terlan-aot-attestation.") as temporary:
        directory = Path(temporary)
        repository = directory / "repository"
        repository.mkdir()
        subprocess.run(["git", "init", "--quiet"], cwd=repository, check=True)
        subprocess.run(
            ["git", "-c", "user.name=Terlan", "-c", "user.email=test@terlan.dev", "commit", "--allow-empty", "-m", "fixture", "--quiet"],
            cwd=repository,
            check=True,
        )
        fixture_revision = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=repository,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        fixture = dict(report)
        fixture["source_revision"] = fixture_revision
        report_path = directory / "report.json"
        output_path = directory / "attestation.json"
        report_path.write_text(json.dumps(fixture), encoding="utf-8")
        record_attestation(report_path, output_path, repository)
        assert json.loads(output_path.read_text(encoding="utf-8")) == fixture
        fixture["source_revision"] = "9" * 40
        report_path.write_text(json.dumps(fixture), encoding="utf-8")
        try:
            record_attestation(report_path, output_path, repository)
        except AssertionError:
            pass
        else:
            raise AssertionError("attestation promotion accepted an unrelated revision")

    with tempfile.TemporaryDirectory(prefix="terlan-aot-roadmap.") as temporary:
        missing_root = Path(temporary)
        try:
            find_roadmap(MAIN_ROADMAP, missing_root)
        except AssertionError:
            pass
        else:
            raise AssertionError("reconciliation accepted a missing roadmap")
    print("TVM AOT roadmap reconciliation self-test passed")


def main() -> int:
    """Dispatch live reconciliation or adversarial contract validation."""

    parser = argparse.ArgumentParser()
    parser.add_argument(
        "command", choices=("attest", "check", "self-test"), nargs="?", default="check"
    )
    parser.add_argument("--report", type=Path)
    arguments = parser.parse_args()
    try:
        if arguments.command == "self-test":
            self_test()
        elif arguments.command == "attest":
            if arguments.report is None:
                raise AssertionError("`attest` requires `--report PATH`")
            record_attestation(arguments.report)
        else:
            if arguments.report is not None:
                raise AssertionError("`--report` is valid only with `attest`")
            check()
    except (AssertionError, json.JSONDecodeError, OSError, subprocess.CalledProcessError) as error:
        print(f"TVM AOT roadmap reconciliation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
