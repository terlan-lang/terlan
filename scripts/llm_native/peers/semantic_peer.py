"""Independent schema/source oracle for live Terlan snapshot gate artifacts."""

import argparse
import hashlib
import json
from importlib.metadata import version
from pathlib import Path

from jsonschema import Draft202012Validator


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def validate(root, snapshot_path, catalog_path):
    assert version("jsonschema") == "4.25.1"
    snapshot, catalog = load(snapshot_path), load(catalog_path)
    fixture_path = root / "tests/fixtures/llm_native/semantic/sources.json"
    fixture = load(fixture_path)
    schemas = root / "compiler/semantic/schemas"
    Draft202012Validator(load(schemas / "snapshot.schema.json")).validate(snapshot)
    entity_validator = Draft202012Validator(load(schemas / "entity.schema.json"))
    expected_sources = sorted(
        ({"path": row["path"],
          "digest": hashlib.sha256(row["contents"].encode("utf-8")).hexdigest(),
          "document_version": row["document_version"]} for row in fixture),
        key=lambda row: row["path"],
    )
    assert snapshot["sources"] == expected_sources
    assert snapshot["analysis"]["status"] == "partial"
    assert snapshot["analysis"]["limitations"]
    assert catalog["schema"] == "terlan.semantic.entity-index.v1"
    assert catalog["snapshot_id"] == snapshot["snapshot_id"]
    assert catalog["status"] == "partial" and catalog["limitations"]
    assert catalog["span_precision"] == "enclosing_syntax_or_file_anchor"
    rows = catalog["entities"]
    assert rows and len({row["entity_id"] for row in rows}) == len(rows)
    paths = {row["path"]: row for row in fixture}
    kinds = {row["kind"] for row in rows}
    assert {"module", "binding", "function", "type", "trait", "pattern"} <= kinds
    entity_by_id = {row["entity_id"]: row for row in rows}
    for row in rows:
        entity_validator.validate(row)
        assert row["snapshot_id"] == snapshot["snapshot_id"]
        origin = row["origin"]
        assert origin["path"] in paths
        assert 0 <= origin["start_byte"] <= origin["end_byte"] <= len(
            paths[origin["path"]]["contents"].encode("utf-8")
        )
        if row["kind"] == "pattern":
            assert row["owner"] in entity_by_id
            assert entity_by_id[row["owner"]]["kind"] in {"function", "trait", "type"}
    assert any(row["kind"] == "trait" and row["origin"]["path"] == "library.terli" for row in rows)
    assert any(row["kind"] == "binding" and row["origin"]["path"] == "main.terl" for row in rows)
    return {
        "schema": "terlan.semantic.live-schema-evidence.v1",
        "status": "passed",
        "snapshot_id": snapshot["snapshot_id"],
        "snapshot_digest": hashlib.sha256(snapshot_path.read_bytes()).hexdigest(),
        "catalog_digest": hashlib.sha256(catalog_path.read_bytes()).hexdigest(),
        "fixture_digest": hashlib.sha256(fixture_path.read_bytes()).hexdigest(),
        "source_count": len(expected_sources),
        "entity_count": len(rows),
        "entity_kinds": sorted(kinds),
        "validator": {"jsonschema": version("jsonschema")},
        "scope": "Public schemas, exact fixture source/version inventory, entity membership and origin bounds; no MCP or edit verification claim.",
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--snapshot", type=Path, required=True)
    parser.add_argument("--catalog", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    try:
        report = validate(args.root.resolve(), args.snapshot, args.catalog)
    except Exception as error:
        report = {"schema": "terlan.semantic.live-schema-evidence.v1", "status": "failed",
                  "error": str(error), "error_type": type(error).__name__}
    with args.output.open("x", encoding="utf-8") as target:
        json.dump(report, target, indent=2, sort_keys=True)
        target.write("\n")
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
