"""Independent MCP SDK stdio peer. No production server implementation lives here."""

import argparse
import asyncio
from datetime import timedelta
import hashlib
from importlib.metadata import version
import json
import os
from pathlib import Path
import subprocess
import time

from jsonschema import Draft202012Validator
from mcp import ClientSession, StdioServerParameters, types
from mcp.client.stdio import stdio_client
from mcp.shared.exceptions import McpError


async def validate(args):
    assert version("mcp") == "1.26.0" and version("jsonschema") == "4.25.1"
    root = args.root.resolve()
    compiler = args.compiler.resolve()
    schemas = root / "compiler/semantic/schemas"
    fixture = args.source_fixture or root / "tests/fixtures/llm_native/mcp/query.terl"
    fixture_digest = hashlib.sha256(fixture.read_bytes()).hexdigest()
    spec_root = root / "tests/fixtures/llm_native/mcp"
    provenance = json.loads((spec_root / "spec.provenance.json").read_text())
    assert provenance["revision"] == "38c84e9f93ad191d9eb26d92b945d17bd0efcaf3"
    for item in provenance["files"]:
        assert hashlib.sha256((spec_root / item["path"]).read_bytes()).hexdigest() == item["sha256"]
    spec_bytes = (spec_root / "spec.schema.json").read_bytes()
    spec_validator = Draft202012Validator(dict(json.loads(spec_bytes), **{"$ref": "#/$defs/JSONRPCMessage"}))
    read = lambda name: json.loads((schemas / (name + ".schema.json")).read_text())
    entity_validator = Draft202012Validator(read("entity"))
    result_validator = Draft202012Validator(read("query-result"))
    errors = Draft202012Validator(read("query-error"))
    cases, artifacts = args.evidence_cases, args.evidence_artifacts
    started = time.monotonic()
    progress = args.output.with_suffix(".progress.jsonl")
    progress.open("x").close()

    def passed(name, evidence):
        row = {"case_id": name, "status": "passed", "evidence": evidence,
               "elapsed_ms": round((time.monotonic() - started) * 1000)}
        cases.append(row)
        with progress.open("a") as target:
            target.write(json.dumps({key: row[key] for key in ("case_id", "status", "elapsed_ms")}) + "\n")

    server_args = ["semantic", "mcp", str(args.store.resolve()), args.snapshot,
                   "--cache-dir", str(args.cache.resolve()),
                   "--out-dir", str(args.cache.resolve() / "build")]
    parameters = StdioServerParameters(command=str(compiler), args=server_args,
                                      cwd=str(root), env=dict(os.environ, TERLAN_RUN_STDIO="1"))
    with args.stderr.open("x") as errlog:
        async with stdio_client(parameters, errlog=errlog) as (incoming, outgoing):
            async with ClientSession(incoming, outgoing, read_timeout_seconds=timedelta(seconds=120)) as client:
                initialized = await client.initialize()
                assert initialized.protocolVersion == "2025-11-25"
                assert initialized.capabilities.tools is not None
                assert initialized.capabilities.resources is not None
                assert not initialized.capabilities.resources.subscribe
                passed("mcp.initialize", initialized.model_dump(mode="json"))
                await client.send_ping()
                passed("mcp.ping", {})

                listed = await client.list_tools()
                assert [tool.name for tool in listed.tools] == ["terlan_query"]
                tool = listed.tools[0]
                assert tool.outputSchema is not None
                assert tool.inputSchema == read("query")
                Draft202012Validator.check_schema(tool.inputSchema)
                Draft202012Validator.check_schema(tool.outputSchema)
                output_validator = Draft202012Validator(tool.outputSchema)
                artifacts["tool"] = tool.model_dump(mode="json")
                passed("mcp.tool-discovery-and-schemas", {"tool": tool.name})

                resources, token, tokens = [], None, set()
                resume_token, resource_pages = None, []
                while True:
                    page = await client.list_resources(cursor=token)
                    resources.extend(page.resources)
                    resource_pages.append([str(row.uri) for row in page.resources])
                    token = page.nextCursor
                    if token is None:
                        break
                    assert token not in tokens
                    tokens.add(token)
                    if resume_token is None:
                        resume_token = token
                if args.require_resource_pagination:
                    assert resume_token is not None and len(resource_pages) > 1
                assert resources and len({str(row.uri) for row in resources}) == len(resources)
                assert all(str(row.uri).startswith("terlan://snapshots/" + args.snapshot + "/") for row in resources)
                passed("mcp.resource-discovery", {"count": len(resources), "continuations": len(tokens)})

                entities, manifest = [], None
                for descriptor in resources:
                    result = await client.read_resource(descriptor.uri)
                    assert len(result.contents) == 1
                    contents = result.contents[0]
                    assert isinstance(contents, types.TextResourceContents)
                    assert len(contents.text.encode()) <= 65536
                    document = json.loads(contents.text)
                    assert document["snapshot_id"] == args.snapshot
                    if str(descriptor.uri).endswith("/manifest"):
                        Draft202012Validator(read("snapshot")).validate(document)
                        manifest = document
                    else:
                        assert document["schema"] == "terlan.semantic.entity-page.v1"
                        assert document["status"] == "partial" and document["limitations"]
                        for entity in document["entities"]:
                            entity_validator.validate(entity)
                        entities.extend(document["entities"])
                assert manifest is not None and entities
                assert len(manifest["sources"]) == 1 and manifest["sources"][0]["digest"] == fixture_digest
                assert len({row["entity_id"] for row in entities}) == len(entities)
                artifacts["entities"] = entities
                passed("mcp.bounded-schema-valid-resources", {"entities": len(entities)})
                templates = await client.list_resource_templates()
                assert templates.resourceTemplates == []
                passed("mcp.empty-template-discovery", {})

                decide = next(row for row in entities if row["kind"] == "function" and row["name"] == "decide")

                def query(entity=decide, fact="type", **overrides):
                    document = {"schema": "terlan.semantic.query.v1", "snapshot_id": args.snapshot,
                                "target": {"entity_id": entity["entity_id"], "kind": entity["kind"],
                                           "expected_digest": entity["digest"]},
                                "fact": fact, "limits": {"max_bytes": 16384, "max_work": 10000}, "cursor": None}
                    document.update(overrides)
                    return document

                async def call(arguments, failed=False):
                    response = await client.call_tool("terlan_query", arguments)
                    assert response.isError is failed
                    assert isinstance(response.structuredContent, dict)
                    document = response.structuredContent
                    output_validator.validate(document)
                    (errors if failed else result_validator).validate(document)
                    assert len(response.content) == 1 and isinstance(response.content[0], types.TextContent)
                    assert json.loads(response.content[0].text) == document
                    return document

                signature = await call(query())
                assert signature["facts"][0]["value"]["return_type"] == "Bool"
                artifacts["query"] = signature
                passed("mcp.compiler-type-query", signature)
                local = next(row for row in entities if row["kind"] == "binding" and row["name"] == "local")
                inferred = await call(query(entity=local, fact="type"))
                local_type = inferred["facts"][0]["value"]
                assert inferred["status"] == "complete" and len(inferred["facts"]) == 1
                assert inferred["facts"][0]["kind"] == "inferred_binding_type"
                assert local_type["compiler_type"] == "Bool" and local_type["display_type"] == "Bool"
                assert local_type["model"] == "checked-binding-type.v1"
                assert local_type["type_encoding"] == "terlan.typecheck.type.v1"
                assert local_type["scope"] == "checked_binding_definition"
                assert local_type["type_variable_scope"] == "observation_local"
                assert local_type["binding"]["name"] == "local"
                assert inferred["facts"][0]["origin_entity_ids"] == [local["entity_id"]]
                passed("mcp.checked-inferred-local-type", inferred)
                declaration = await call(query(fact="declaration"))
                assert declaration["facts"][0]["value"]["payload"]["name"] == "decide"
                definition = await call(query(fact="definition"))
                assert definition["facts"][0]["value"]["entity"] == decide
                assert definition["facts"][0]["value"]["range_precision"] == "enclosing_syntax"
                interface = await call(query(fact="interface"))
                interface_value = interface["facts"][0]["value"]
                assert "decide(" in interface_value["source"]
                assert hashlib.sha256(interface_value["source"].encode()).hexdigest() == interface_value["digest"]
                core = await call(query(fact="core"))
                core_value = core["facts"][0]["value"]
                assert hashlib.sha256(core_value["text"].encode()).hexdigest() == core_value["digest"]
                assert json.loads(core_value["text"])["source"]["function"] == "decide"
                passed("mcp.declaration-definition-interface-core", {"declaration": declaration, "definition": definition, "interface": interface, "core": core})
                references = await call(query(entity=local, fact="references"))
                assert references["status"] == "partial" and len(references["facts"]) == 1
                assert references["facts"][0]["kind"] == "binding_reference"
                assert references["facts"][0]["value"]["name"] == "local"
                assert references["facts"][0]["origin_entity_ids"] == [local["entity_id"]]
                passed("mcp.lexical-binding-references", references)
                candidates = await call(query(entity=local, fact="legal_transformations"))
                candidate = candidates["facts"][0]["value"]
                assert candidates["status"] == "partial" and candidate["candidate_set_exhaustive"] is False
                assert candidate["availability"] == "planned" and candidate["applicability"] == "requires_verification"
                assert candidate["constraints"] and candidate["verification_required"]
                passed("mcp.transformation-candidates-require-verification", candidates)
                effects = await call(query(fact="effects"))
                purity = effects["facts"][0]["value"]
                assert purity["contract"]["compiler_proven_pure"] is True
                assert purity["scope"] == "checked_interface_purity"
                assert purity["effect_set_exhaustive"] is False and effects["status"] == "partial"
                passed("mcp.checked-purity-contract", effects)
                coverage = await call(query(fact="pattern_coverage"))
                observation = next(row["value"] for row in coverage["facts"] if row["value"]["subject_kind"] == "function_clauses")
                assert observation["owner"]["name"] == "decide"
                assert observation["exhaustive"] is True and observation["consistent"] is True
                assert observation["uncovered"] == [] and coverage["status"] == "partial"
                passed("mcp.checked-pattern-coverage", coverage)
                expected = await call(query(fact="expected_values"))
                value = expected["facts"][0]["value"]
                assert value["literal_candidates"] == [False, True]
                assert value["exhaustive_for_type"] and not value["exhaustive_for_program_location"]
                assert value["verification_required"] and expected["status"] == "partial"
                passed("mcp.expected-values-constraints", expected)

                choices = [row for row in entities if row["kind"] == "function" and row["name"] == "choose"]
                assert len(choices) == 2
                signatures = [await call(query(entity=row)) for row in choices]
                assert {row["facts"][0]["value"]["return_type"] for row in signatures} == {"Int", "String"}
                passed("mcp.same-arity-overload-identity", {"entities": [row["entity_id"] for row in choices]})
                for chosen in choices:
                    references = await call(query(entity=chosen, fact="references"))
                    assert references["status"] == "partial" and len(references["facts"]) == 1
                    reference = references["facts"][0]["value"]
                    assert references["facts"][0]["kind"] == "checked_call_reference"
                    assert reference["scope"] == "checked_calls_in_snapshot"
                    assert reference["observation"]["status"] == "resolved"
                    assert reference["target_origin"]["entity"] == chosen
                    assert reference["source_path"] == manifest["sources"][0]["path"]
                passed("mcp.checked-overload-call-references", {"selected_entities": choices})
                module_entity = next(row for row in entities if row["kind"] == "module")
                call_inventory = await call(query(entity=module_entity, fact="references"))
                indirect = next(row["value"] for row in call_inventory["facts"] if "function:invoke_dynamic/1" in row["value"]["observation"]["site"]["path"])
                assert indirect["scope"] == "selected_module_calls"
                assert indirect["observation"]["status"] == "unavailable" and indirect["observation"]["target"] is None
                assert indirect["target_origin"]["status"] == "unavailable" and indirect["target_origin"]["entity"] is None
                assert call_inventory["status"] == "partial" and call_inventory["limitations"]
                passed("mcp.explicit-indirect-call-target", call_inventory)
                leaf = next(row for row in entities if row["kind"] == "type" and row["name"] == "Leaf")
                base = next(row for row in entities if row["kind"] == "type" and row["name"] == "Base")
                structure = await call(query(entity=leaf, fact="type"))
                structure_value = structure["facts"][0]["value"]
                assert structure_value["scope"] == "checked_type_declaration"
                assert structure_value["included_origins"][0] == base
                assert len(structure_value["source_declaration"]["fields"]) == 2
                passed("mcp.included-type-origin", structure)
                pattern = next(row for row in entities if row["kind"] == "pattern" and row["owner"] == decide["entity_id"])
                pattern_core = await call(query(entity=pattern, fact="core"))
                pattern_value = pattern_core["facts"][0]["value"]
                assert pattern_value["scope"] == "enclosing_function"
                assert pattern_value["owner_entity_id"] == decide["entity_id"]
                assert json.loads(pattern_value["text"])["source"]["function"] == "decide"
                passed("mcp.pattern-owner-core-origin", pattern_core)
                shape = next(row for row in entities if row["kind"] == "shape" and row["name"] == "Positive")
                shape_result = await call(query(entity=shape, fact="conformance"))
                shape_fact = next(row["value"] for row in shape_result["facts"] if row["value"]["relation_kind"] == "shape_definition")
                assert shape_fact["evidence"]["pattern"]["kind"] == "var"
                assert shape_fact["evidence"]["guard"]["kind"] == "binary_op"
                assert shape_fact["candidate_set_exhaustive"] is False and shape_result["status"] == "partial"
                passed("mcp.checked-shape-definition", shape_result)
                trait = next(row for row in entities if row["kind"] == "trait" and row["name"] == "Encode")
                trait_result = await call(query(entity=trait, fact="conformance"))
                definition = next(row["value"]["evidence"] for row in trait_result["facts"] if row["value"]["relation_kind"] == "trait_definition")
                negative = next(row["value"] for row in trait_result["facts"] if row["value"]["relation_kind"] == "trait_conformance" and row["value"]["evidence"]["is_negative"])
                assert definition["methods"][0]["name"] == "encode"
                assert negative["scope"] == "containing_module" and negative["evidence"]["is_negative"] is True
                passed("mcp.negative-trait-conformance", trait_result)
                positive = next(row["value"] for row in trait_result["facts"] if row["value"]["relation_kind"] == "trait_conformance" and not row["value"]["evidence"]["is_negative"])
                assert positive["scope"] == "containing_module"
                assert positive["evidence"]["source"] == "explicit_impl" and positive["evidence"]["for_type"] == "Int"
                assert positive["evidence"]["is_negative"] is False
                passed("mcp.positive-trait-conformance", positive)
                display = next(row for row in entities if row["kind"] == "function" and row["name"] == "display")
                implication = await call(query(entity=display, fact="conformance"))
                contract = next(row["value"]["evidence"] for row in implication["facts"] if row["value"]["relation_kind"] == "declaration_requirements")
                assert contract["type_variables"][0]["name"] == "T"
                assert contract["bounds"][0]["kind"] == "structural_implication"
                assert contract["bounds"][0]["required_shape"]["Map"][0] == {"key": "name", "value": "Binary", "required": True}
                assert implication["status"] == "partial"
                passed("mcp.checked-implication-requirement", implication)
                bound_owner = next(row for row in entities if row["kind"] == "function" and row["name"] == "encode_bound")
                bounded = await call(query(entity=bound_owner, fact="conformance"))
                bound_fact = next(row["value"] for row in bounded["facts"] if row["value"]["relation_kind"] == "declaration_requirements")
                origin = bound_fact["bound_origins"][0]
                entity_validator.validate(origin["entity"])
                assert origin["status"] == "resolved" and origin["entity"] == trait
                passed("mcp.checked-bound-provider-origin", bounded)
                positive_owner = next(row for row in entities if row["kind"] == "function" and row["name"] == "positive")
                expanded_pattern = None
                for candidate in [row for row in entities if row["kind"] == "pattern" and row["owner"] == positive_owner["entity_id"]]:
                    declaration = await call(query(entity=candidate, fact="declaration"))
                    if declaration["facts"][0]["value"]["payload"].get("expanded_from"):
                        expanded_pattern = candidate
                        break
                assert expanded_pattern is not None
                expanded = await call(query(entity=expanded_pattern, fact="conformance"))
                expansion = expanded["facts"][0]["value"]
                assert expansion["relation_kind"] == "shape_expansion" and expansion["scope"] == "selected_pattern"
                entity_validator.validate(expansion["evidence"]["entity"])
                assert expansion["evidence"]["status"] == "resolved" and expansion["evidence"]["entity"] == shape
                passed("mcp.checked-shape-expansion-origin", expanded)
                unavailable = await call(query(fact="ownership"))
                assert unavailable["status"] == "unavailable" and not unavailable["facts"] and unavailable["limitations"]
                passed("mcp.explicit-unknown", unavailable)

                stale = query()
                stale["target"]["expected_digest"] = "0" * 64
                rejected = await call(stale, failed=True)
                assert rejected["code"] == "semantic.entity.stale"
                passed("mcp.stale-entity-rejection", rejected)
                scoped = await call(query(snapshot_id="s:" + "0" * 64), failed=True)
                assert scoped["code"] == "semantic.mcp.snapshot_scope"
                passed("mcp.snapshot-scope-rejection", scoped)
                invalid = await call(query(limits={"max_bytes": 0, "max_work": 1000}), failed=True)
                passed("mcp.invalid-budget-tool-error", invalid)

                try:
                    await client.call_tool("unknown.tool", {})
                    raise AssertionError("Unknown tool accepted")
                except McpError as error:
                    assert error.error.code == -32602
                passed("mcp.unknown-tool-protocol-error", {"code": -32602})
                try:
                    await client.list_resources(cursor="c:" + "0" * 64)
                    raise AssertionError("Forged resource continuation accepted")
                except McpError as error:
                    assert error.error.code == -32602
                passed("mcp.forged-resource-cursor", {"code": -32602})
                try:
                    await client.read_resource("file:///etc/passwd")
                    raise AssertionError("Unadmitted URI accepted")
                except McpError as error:
                    assert error.error.code == -32002
                passed("mcp.resource-uri-confinement", {"code": -32002})

    if args.require_resource_pagination:
        with args.stderr.open("a") as errlog:
            async with stdio_client(parameters, errlog=errlog) as (incoming, outgoing):
                async with ClientSession(incoming, outgoing, read_timeout_seconds=timedelta(seconds=120)) as client:
                    await client.initialize()
                    continued = await client.list_resources(cursor=resume_token)
                    assert [str(row.uri) for row in continued.resources] == resource_pages[1]
        passed("mcp.resource-continuation-survives-reconnect", {"continuation": resume_token,
               "first_connection_pages": len(resource_pages)})

    request_path = args.output.with_suffix(".query-request.json")
    with request_path.open("x") as target:
        json.dump(query(), target)
    cli = await asyncio.to_thread(
        subprocess.run,
        [str(compiler), "semantic", "query", str(args.store.resolve()), str(request_path.resolve()),
         "--cache-dir", str(args.cache.resolve()), "--out-dir", str(args.cache.resolve() / "build")],
        cwd=root, stdin=subprocess.DEVNULL, capture_output=True, timeout=120,
    )
    assert cli.returncode == 0, cli.stderr.decode(errors="replace")
    assert json.loads(cli.stdout) == signature
    passed("mcp.cli-query-parity", {"request_digest": hashlib.sha256(request_path.read_bytes()).hexdigest()})

    # Raw standard JSON-RPC probes cover frames intentionally rejected by the
    # SDK's request constructors; the SDK still validates every server reply.
    initialize = {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-11-25", "capabilities": {},
        "clientInfo": {"name": "raw-wire-peer", "version": "1"}}}
    large_id = 18446744073709551615
    messages = [
        b"{not-json",
        {"jsonrpc": "2.0", "id": 0, "method": "tools/list"}, initialize,
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "terlan_query", "arguments": []}},
        {"jsonrpc": "2.0", "id": 3, "method": "resources/read", "params": {}},
        {"jsonrpc": "1.0", "id": 4, "method": "ping"},
        {"jsonrpc": "2.0", "id": large_id, "method": "ping"},
        {"jsonrpc": "2.0", "id": large_id, "method": "ping"},
        {"jsonrpc": "2.0", "id": "missing-method", "method": "not-a-server-method"},
        {"jsonrpc": "2.0", "method": "unknown-notification"},
        {"jsonrpc": "2.0", "id": "unsolicited-response", "result": {}},
        {"jsonrpc": "2.0", "error": {"code": -32700, "message": "Invalid JSON."}},
    ]
    wire = b"\r\n".join(value if isinstance(value, bytes) else json.dumps(value).encode() for value in messages) + b"\r\n"
    raw = await asyncio.to_thread(subprocess.run, [str(compiler), *server_args],
                                 cwd=root, input=wire, capture_output=True, timeout=180)
    assert raw.returncode == 0, raw.stderr.decode(errors="replace")
    replies = [json.loads(line) for line in raw.stdout.splitlines()]
    assert len(replies) == 9
    for reply in replies:
        spec_validator.validate(reply)
        if "id" in reply:
            types.JSONRPCMessage.model_validate(reply, strict=True)
    assert "id" not in replies[0]
    assert [(row.get("id"), row.get("error", {}).get("code")) for row in replies] == [
        (None, -32700), (0, -32002), (1, None), (2, -32602), (3, -32602), (4, -32600),
        (large_id, None), (large_id, -32600), ("missing-method", -32601),
    ]
    types.InitializeResult.model_validate(replies[2]["result"], strict=True)
    assert replies[6]["result"] == {}
    artifacts["raw_wire_replies"] = replies
    passed("mcp.raw-wire-framing-and-errors", {"requests_and_notifications": len(messages), "replies": len(replies),
           "frame_terminator": "CRLF", "eof_exit_code": raw.returncode})

    return {"schema": "terlan.semantic.mcp-peer-evidence.v1", "status": "passed",
            "protocol_version": "2025-11-25", "snapshot_id": args.snapshot,
            "fixture_digest": fixture_digest,
            "protocol_schema_digest": hashlib.sha256(spec_bytes).hexdigest(),
            "compiler_digest": hashlib.sha256(compiler.read_bytes()).hexdigest(),
            "peer_versions": {"mcp": version("mcp"), "jsonschema": version("jsonschema")},
            "command": [str(compiler), *server_args], "cases": cases, "artifacts": artifacts,
            "scope": "Live independent SDK initialization, discovery, resources, compiler queries and rejection paths; not the full LNC-005 gate."}


def main():
    parser = argparse.ArgumentParser()
    for name in ("root", "compiler", "store", "cache", "output", "stderr"):
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--snapshot", required=True)
    parser.add_argument("--source-fixture", type=Path)
    parser.add_argument("--require-resource-pagination", action="store_true")
    args = parser.parse_args()
    args.evidence_cases, args.evidence_artifacts = [], {}
    try:
        report = asyncio.run(validate(args))
    except Exception as error:
        report = {"schema": "terlan.semantic.mcp-peer-evidence.v1", "status": "failed",
                  "error": str(error), "error_type": type(error).__name__,
                  "cases": args.evidence_cases, "artifacts": args.evidence_artifacts}
        import traceback
        traceback.print_exc()
    with args.output.open("x") as target:
        json.dump(report, target, indent=2, sort_keys=True)
        target.write("\n")
    print(json.dumps({"status": report["status"], "cases": len(report.get("cases", []))}))
    return 0 if report["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
