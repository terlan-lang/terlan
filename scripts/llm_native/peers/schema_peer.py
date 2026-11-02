"""Independent JSON Schema oracle for LNC-001, invoked by the Terlan gate."""

import argparse
import hashlib
import json
from importlib.metadata import version
from pathlib import Path

from jsonschema import Draft202012Validator
from mcp import types
from pydantic import ValidationError


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def pointer(parts):
    return "".join("/" + str(p).replace("~", "~0").replace("/", "~1") for p in parts)


def validate(root):
    contract_path = root / "compiler/semantic/contract.json"
    contract = load(contract_path)
    assert contract["schema"] == "terlan.semantic.service-contract.v1"
    assert contract["mcp_protocol_version"] == types.LATEST_PROTOCOL_VERSION
    assert version("jsonschema") == "4.25.1" and version("mcp") == "1.26.0"
    schemas = {}
    inputs = [contract_path]
    spec_root = root / "tests/fixtures/llm_native/mcp"
    spec_path = spec_root / "spec.schema.json"
    provenance_path = spec_root / "spec.provenance.json"
    provenance = load(provenance_path)
    assert provenance["revision"] == "38c84e9f93ad191d9eb26d92b945d17bd0efcaf3"
    for item in provenance["files"]:
        path = spec_root / item["path"]
        assert hashlib.sha256(path.read_bytes()).hexdigest() == item["sha256"]
        inputs.append(path)
    inputs.append(provenance_path)
    specification = load(spec_path)
    Draft202012Validator.check_schema(specification)
    spec_validator = Draft202012Validator(dict(specification, **{"$ref": "#/$defs/JSONRPCMessage"}))
    for item in contract["documents"]:
        path = (contract_path.parent / item["path"]).resolve()
        assert path.is_relative_to((root / "compiler/semantic/schemas").resolve())
        schema = load(path)
        assert schema["$id"] == item["schema_id"]
        assert schema["$schema"] == contract["json_schema_dialect"]
        assert item["name"] not in schemas
        Draft202012Validator.check_schema(schema)
        schemas[item["name"]] = Draft202012Validator(schema)
        inputs.append(path)
    assert set(schemas) == {
        "snapshot", "entity", "query", "query-result", "proposal", "diagnostic",
        "verification", "transaction", "provenance", "query-error",
    }
    inventory_path = root / "tests/fixtures/llm_native/contracts/cases.json"
    inventory = load(inventory_path)
    assert inventory["schema"] == "terlan.semantic.contract-cases.v1"
    assert inventory["cases"]
    inputs.append(inventory_path)
    seen, covered = set(), set()
    rows = []
    for case in inventory["cases"]:
        assert case["id"] not in seen
        seen.add(case["id"])
        assert case["schema"] in schemas
        errors = sorted(
            schemas[case["schema"]].iter_errors(case["document"]),
            key=lambda e: (pointer(e.absolute_path), pointer(e.absolute_schema_path)),
        )
        diagnostics = [
            {"code": "semantic.schema." + str(e.validator),
             "instance_pointer": pointer(e.absolute_path),
             "schema_pointer": pointer(e.absolute_schema_path), "message": e.message}
            for e in errors
        ]
        expected = case["expected_diagnostic"]
        passed = not errors if expected is None else any(d["code"] == expected for d in diagnostics)
        covered.add((case["schema"], expected is None))
        rows.append({"case_id": case["id"], "status": "passed" if passed else "failed",
                     "expected_diagnostic": expected, "diagnostics": diagnostics})
    assert covered == {(name, accepted) for name in schemas for accepted in (True, False)}
    wire_path = root / "tests/fixtures/llm_native/mcp/wire-cases.json"
    wire = load(wire_path)
    assert wire["schema"] == "terlan.mcp.wire-cases.v1"
    assert wire["protocol_version"] == contract["mcp_protocol_version"]
    assert wire["cases"]
    inputs.append(wire_path)
    wire_rows, wire_seen = [], set()
    models = {name: getattr(types, name) for name in
              ("JSONRPCMessage", "Tool", "CallToolResult", "ReadResourceResult")}
    for case in wire["cases"]:
        assert case["id"] not in wire_seen
        wire_seen.add(case["id"])
        try:
            if case["model"] == "McpSpecMessage":
                accepted = spec_validator.is_valid(case["message"])
            else:
                models[case["model"]].model_validate(case["message"], strict=True)
                accepted = True
        except ValidationError:
            accepted = False
        wire_rows.append({"case_id": case["id"],
                          "status": "passed" if accepted == case["accept"] else "failed"})
    # The SDK validates the standard tool descriptor, independently of our server.
    # These descriptors are test fixtures and are never advertised as live tools.
    types.Tool.model_validate({
        "name": "terlan_query", "description": "Query a checked compiler snapshot.",
        "inputSchema": schemas["query"].schema,
        "outputSchema": schemas["query-result"].schema,
    })
    types.Tool.model_validate({
        "name": "terlan_prepare", "description": "Prepare an isolated semantic edit.",
        "inputSchema": schemas["proposal"].schema,
        "outputSchema": schemas["transaction"].schema,
    })
    inputs += [Path(__file__).resolve(), Path(__file__).with_name("requirements.txt").resolve()]
    digest = hashlib.sha256()
    input_rows = []
    for path in sorted(inputs):
        label = path.relative_to(root).as_posix()
        file_digest = hashlib.sha256(path.read_bytes()).hexdigest()
        for field in (label.encode(), file_digest.encode()):
            digest.update(len(field).to_bytes(8, "big"))
            digest.update(field)
        input_rows.append({"path": label, "digest": file_digest})
    return {
        "schema": "terlan.semantic.contract-evidence.v1", "task": "LNC-001",
        "kind": "schema_validation", "source_closure_digest": digest.hexdigest(),
        "peer_versions": {"jsonschema": version("jsonschema"), "mcp": version("mcp")},
        "mcp_protocol_version": contract["mcp_protocol_version"],
        "status": "passed" if all(r["status"] == "passed" for r in rows + wire_rows) else "failed",
        "case_count": len(rows), "inputs": input_rows, "cases": rows,
        "wire_case_count": len(wire_rows), "wire_cases": wire_rows,
        "wire_sdk_case_count": sum(case["model"] != "McpSpecMessage" for case in wire["cases"]),
        "wire_spec_case_count": sum(case["model"] == "McpSpecMessage" for case in wire["cases"]),
        "limitations": ["Schema acceptance is not runtime/compiler verification.",
                         "Tool descriptors are SDK-validated fixtures, not server interoperability.",
                         "SDK 1.26.0 requires an error ID; the official schema validates anonymous errors permitted by MCP."],
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    report = validate(args.root.resolve())
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(report, output, indent=2, sort_keys=True)
        output.write("\n")
    print(json.dumps({"status": report["status"], "cases": report["case_count"]}))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
