use super::*;

fn checked(source: &str) -> Value {
    let (value, status) = export("sample.terl", source, &CliState::default()).unwrap();
    assert_eq!(status, ExitCode::SUCCESS);
    assert_eq!(value["status"], "checked");
    value
}

#[test]
fn exports_checked_core_and_exact_source_identity() {
    let source = "module sample.\npub value(input: Int): Int -> let local = input + 1; local.\n";
    let value = checked(source);
    assert_eq!(value["source_digest"], digest(source));
    assert_eq!(value["core"]["schema"], "terlan.core_ir.v1");
    assert_eq!(value["core"]["module"], "sample");
    assert!(!value["source_declarations"].as_array().unwrap().is_empty());
    assert!(value["core"]["binding_identities"]["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|entry| entry["name"] == "local"));
}

fn calls(value: &Value) -> &[Value] {
    assert_eq!(value["call_resolution_observations"]["truncated"], false);
    value["call_resolution_observations"]["calls"]
        .as_array()
        .unwrap()
}

#[test]
fn accepted_local_overloads_retain_exact_selected_declaration_spans() {
    let value = checked("module sample.\npub pick(input: Int): Int -> input.\npub pick(input: Bool): Bool -> input.\npub number(): Int -> pick(1).\npub flag(): Bool -> pick(true).\n");
    let rows = calls(&value);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows.iter().all(|r| r["status"] == "resolved"));
    let chosen = rows
        .iter()
        .map(|r| r["target"]["span_start"].as_u64().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    let declarations = value["source_declarations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["payload"]["name"] == "pick")
        .map(|d| d["span"]["start"].as_u64().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(chosen, declarations);
    assert!(rows
        .iter()
        .all(|r| r["target"]["identity_kind"] == "checked_source_declaration"));
}

#[test]
fn selected_alias_and_module_calls_keep_the_accepted_interface_overload() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("fixture")).unwrap();
    std::fs::write(
        root.path().join("fixture/Provider.terli"),
        "module fixture.Provider.\npub pick(input: Int): Int.\npub pick(input: Bool): Bool.\n",
    )
    .unwrap();
    std::fs::write(root.path().join("sample.terl"), "module sample.\nimport fixture.Provider.{pick as alias}.\nimport fixture.Provider as Other.\npub number(): Int -> alias(1).\npub flag(): Bool -> Other.pick(true).\n").unwrap();
    let (bundle, status) = export_sources(root.path(), &CliState::default()).unwrap();
    assert_eq!(status, ExitCode::SUCCESS);
    let caller = bundle["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| &u["export"])
        .find(|a| a["module_name"] == "sample")
        .unwrap();
    let rows = calls(caller);
    assert_eq!(rows.len(), 2, "{rows:?}");
    let selected = rows
        .iter()
        .map(|r| r["target"]["signature"]["parameter_types"].clone())
        .collect::<Vec<_>>();
    assert!(selected.contains(&json!(["Int"])) && selected.contains(&json!(["Bool"])));
    assert!(rows.iter().all(|r| r["status"] == "resolved"
        && r["target"]["signature"]["module"] == "fixture.Provider"
        && r["target"]["signature"]["name"] == "pick"));
}

#[test]
fn a_function_value_shadowing_a_function_name_stays_explicitly_unresolved() {
    let value = checked("module sample.\npub pick(input: Int): Int -> input.\npub invoke(pick: (Int) -> Int): Int -> pick(1).\n");
    let rows = calls(&value);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["status"], "unavailable");
    assert!(rows[0]["target"].is_null());
    assert_eq!(rows[0]["reason"], "no_unique_exported_static_target");
}

#[test]
fn pipe_selection_records_the_selected_target_without_inventing_a_call_site() {
    let value = checked(
        "module sample.\npub pick(input: Int): Int -> input.\npub invoke(): Int -> pick(1).\n",
    );
    let rows = calls(&value);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["status"], "resolved");
    assert_eq!(rows[0]["target"]["name"], "pick");
    assert!(rows[0]["site"]["path"]
        .as_str()
        .unwrap()
        .contains("function:invoke/0"));
}

fn binding_types(value: &Value, name: &str) -> Vec<Value> {
    assert_eq!(value["binding_type_observations"]["truncated"], false);
    let identities = value["core"]["binding_identities"]["bindings"]
        .as_array()
        .unwrap();
    value["binding_type_observations"]["bindings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["binding"]["name"] == name)
        .map(|row| {
            assert_eq!(row["module"], value["core"]["module"]);
            assert!(
                identities.contains(&row["binding"]),
                "not an exact Core binding: {row}"
            );
            row.clone()
        })
        .collect()
}

#[test]
fn inferred_binding_types_join_exact_lexical_identities() {
    let value =
        checked("module sample.\npub value(input: Int): Int -> let local = input + 1; local.\n");
    let rows = binding_types(&value, "local");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["compiler_type"], "Int");
    assert_eq!(rows[0]["display_type"], "Int");
    let next = checked("module sample.\npub value(): Bool -> true.\n");
    assert!(binding_types(&next, "local").is_empty());
}

#[test]
fn inferred_binding_types_preserve_shadowed_names() {
    let value = checked("module sample.\npub value(input: Int): Int -> let local = input + 1; let nested = (let local = true; local); local.\n");
    let rows = binding_types(&value, "local");
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_ne!(rows[0]["binding"]["id"], rows[1]["binding"]["id"]);
    let types = rows
        .iter()
        .map(|row| row["display_type"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(types, ["Bool", "Int"].into_iter().collect());
}

#[test]
fn inferred_binding_types_follow_destructuring_and_case_constraints() {
    let value = checked("module sample.\npub value(input: {Int, Bool}): Int -> case input { {number, flag} -> case flag { true -> let result = number + 1; result; false -> number } }.\n");
    for (name, expected) in [("number", "Int"), ("flag", "Bool"), ("result", "Int")] {
        let rows = binding_types(&value, name);
        assert_eq!(rows.len(), 1, "{name}: {rows:?}");
        assert_eq!(rows[0]["display_type"], expected);
    }
}

#[test]
fn inferred_binding_types_distinguish_overloads_and_keep_generics_scoped() {
    let value = checked("module sample.\npub value(input: Int): Int -> let local = input; local.\npub value(input: Bool): Bool -> let local = input; local.\npub identity[T](input: T): T -> let copied = input; copied.\n");
    let rows = binding_types(&value, "local");
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_ne!(rows[0]["binding"]["id"], rows[1]["binding"]["id"]);
    let copied = binding_types(&value, "copied");
    assert_eq!(copied.len(), 1);
    assert_eq!(copied[0]["compiler_type"], json!({"Var": 0}));
    assert_eq!(copied[0]["type_variable_scope"], "observation_local");
}

#[test]
fn inferred_binding_types_cover_lambda_generator_and_handler_scopes() {
    let value = checked("module sample.\npub callback(): Term -> (item: Int) -> let saved = item; saved.\npub list(items: List[Int]): List[Int] -> [element | element <- items].\npub handler(input: Int): Int -> try input { result -> result catch _ -> 0 }.\n");
    for name in ["item", "saved", "element", "result"] {
        let rows = binding_types(&value, name);
        assert_eq!(rows.len(), 1, "{name}: {rows:?}");
        assert_eq!(rows[0]["display_type"], "Int");
    }
}

#[test]
fn purity_contracts_are_checked_and_distinguish_overloads() {
    let value = checked(
        "module sample.\nimport std.io.Console.\n@pure\npub pick(value: Int): Int -> value.\npub pick(value: String): Int -> let printed = Console.println(value); 0.\n",
    );
    let contracts = value["purity_contracts"].as_array().unwrap();
    assert_eq!(contracts.len(), 2);
    let pure = contracts
        .iter()
        .find(|row| row["parameter_types"] == json!(["Int"]))
        .unwrap();
    let io = contracts
        .iter()
        .find(|row| row["parameter_types"] == json!(["String"]))
        .unwrap();
    assert_eq!(pure["compiler_proven_pure"], true);
    assert_eq!(io["compiler_proven_pure"], false);
}

#[test]
fn coverage_observations_share_the_checked_finite_verdict() {
    let value = checked(
        "module sample.\npub value(flag: Bool): Bool -> case flag { true where false -> false; true -> true; false -> false }.\n",
    );
    let coverage = &value["coverage_observations"];
    assert_eq!(coverage["model"], "finite-pattern-coverage.v1");
    assert_eq!(coverage["truncated"], false);
    let cases = coverage["observations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["subject_kind"] == "case")
        .collect::<Vec<_>>();
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0]["owner"]["module"], "sample");
    assert_eq!(cases[0]["owner"]["name"], "value");
    assert_eq!(cases[0]["exhaustive"], true);
    assert_eq!(cases[0]["consistent"], true);
    assert_eq!(cases[0]["uncovered"], json!([]));
    assert_eq!(cases[0]["guarded_clauses"], json!([0]));
    assert_eq!(
        cases[0]["subject_syntax_digest"].as_str().unwrap().len(),
        64
    );
    // Capture is per export, never inherited from the previous source.
    let next = checked("module sample.\npub number(): Int -> 1.\n");
    assert!(!next["coverage_observations"]["observations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["subject_kind"] == "case"));
}

#[test]
fn incomplete_function_head_exports_its_actual_missing_variants() {
    let value = checked("module sample.\npub value({true, _}: {Bool, Bool}): Bool -> true.\n");
    let rows = value["coverage_observations"]["observations"]
        .as_array()
        .unwrap();
    let function = rows
        .iter()
        .find(|row| row["subject_kind"] == "function_clauses")
        .unwrap();
    assert_eq!(function["exhaustive"], false);
    assert_eq!(function["guarded_clauses"], json!([]));
    assert_eq!(function["uncovered"], json!(["(false, Bool)"]));
    assert!(value["diagnostics"].as_array().unwrap().iter().any(|row| {
        row["diagnostic"]["message"]
            .as_str()
            .unwrap()
            .contains("non-exhaustive function")
    }));
}

#[test]
fn rejected_case_observations_are_not_successful_core() {
    let (value, status) = export(
        "sample.terl",
        "module sample.\npub value(flag: Bool): Bool -> case flag { true -> true }.\n",
        &CliState::default(),
    )
    .unwrap();
    assert_ne!(status, ExitCode::SUCCESS);
    assert_eq!(value["status"], "invalid");
    assert!(value["core"].is_null());
    assert!(value["purity_contracts"].is_null());
    let rows = value["coverage_observations"]["observations"]
        .as_array()
        .unwrap();
    let case = rows
        .iter()
        .find(|row| row["subject_kind"] == "case")
        .unwrap();
    assert_eq!(case["exhaustive"], false);
    assert_eq!(case["uncovered"], json!(["false"]));
}

#[test]
fn checked_shapes_export_canonical_patterns_guards_and_origins() {
    let value = checked(
        "module sample.\npub shape Positive(value) = value where value > 0.\npub identity(value: Int): Int -> value.\n",
    );
    let declaration = value["source_declarations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["payload"]["raw_kind"] == "shape")
        .unwrap();
    let contract = &declaration["payload"]["shape_contract"];
    assert_eq!(contract["name"], "Positive");
    assert_eq!(contract["params"], json!(["value"]));
    assert_eq!(contract["pattern"]["kind"], "var");
    assert_eq!(contract["guard"]["kind"], "binary_op");
    assert_eq!(contract["public"], true);
    assert!(declaration["span"]["start"].as_u64().unwrap() > 0);
}

#[test]
fn requirements_serialize_the_actual_checked_implication_types() {
    let value = checked(
        "module sample.\npub type Named[T => {name: String}] = T.\npub display[T => {name: String, nested: {code: Int}}](value: T): String -> value.name.\n",
    );
    let rows = value["requirement_observations"]["requirements"]
        .as_array()
        .unwrap();
    let function = rows.iter().find(|row| row["name"] == "display").unwrap();
    assert_eq!(function["type_variables"][0]["name"], "T");
    let bound = &function["bounds"][0];
    assert_eq!(bound["kind"], "structural_implication");
    assert_eq!(bound["subject"], json!({"Var":0}));
    let fields = bound["required_shape"]["Map"].as_array().unwrap();
    assert_eq!(
        fields[0],
        json!({"key":"name","value":"Binary","required":true})
    );
    assert_eq!(fields[1]["value"]["Map"][0]["key"], "code");
    let alias = rows.iter().find(|row| row["name"] == "Named").unwrap();
    assert_eq!(alias["kind"], "type");
    assert_eq!(alias["bounds"][0]["kind"], "structural_implication");
}

#[test]
fn checked_conformances_preserve_explicit_negative_evidence() {
    let value = checked(
        "module sample.\npub trait Encode[T] { encode(value: T): String. }.\npub opaque type Secret.\npub impl not Encode[Secret].\n",
    );
    let facts = value["conformance_contracts"].as_array().unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0]["is_negative"], true);
    assert_eq!(facts[0]["source"], "explicit_impl");
    assert!(value["core"]["trait_conformances"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn local_nested_shape_expansion_retains_canonical_declaration_origins() {
    let value = checked("module sample.\npub shape Wrapped(value) = {Atom[\"wrap\"], value}.\npub shape Nested(value) = Wrapped(value).\npub read(value: {Atom[\"wrap\"], Int}): Int -> case value { Nested(item) -> item }.\n");
    let declaration = value["source_declarations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["payload"]["name"] == "read")
        .unwrap();
    let pattern = &declaration["payload"]["clauses"][0]["body"]["clauses"][0]["patterns"][0];
    assert_eq!(
        pattern["expanded_from"],
        json!([
            {"source_module":"sample","source_name":"Nested"},
            {"source_module":"sample","source_name":"Wrapped"}
        ])
    );
}

#[test]
fn imported_nested_shape_aliases_retain_provider_origins() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("provider.terli"), "module provider.\npub shape Wrapped(value) = {Atom[\"wrap\"], value}.\npub shape Nested(value) = Wrapped(value).\n").unwrap();
    std::fs::write(root.path().join("sample.terl"), "module sample.\nimport provider.{Nested as Alias}.\npub read(value: {Atom[\"wrap\"], Int}): Int -> case value { Alias(item) -> item }.\n").unwrap();
    let (bundle, status) = export_sources(root.path(), &CliState::default()).unwrap();
    assert_eq!(status, ExitCode::SUCCESS);
    let value = &bundle["units"][1]["export"];
    let declaration = value["source_declarations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["payload"]["name"] == "read")
        .unwrap();
    assert_eq!(
        declaration["payload"]["clauses"][0]["body"]["clauses"][0]["patterns"][0]["expanded_from"],
        json!([
            {"source_module":"provider","source_name":"Nested"},
            {"source_module":"provider","source_name":"Wrapped"}
        ])
    );
}

#[test]
fn invalid_source_never_exports_successful_core() {
    let (value, status) = export(
        "sample.terl",
        "module sample.\npub value(): Bool -> 1.\n",
        &CliState::default(),
    )
    .unwrap();
    assert_ne!(status, ExitCode::SUCCESS);
    assert_eq!(value["status"], "invalid");
    assert!(value["core"].is_null());
    assert!(!value["diagnostics"].as_array().unwrap().is_empty());
}

#[test]
fn unrelated_declaration_preserves_exported_binding_identity() {
    let body = "pub value(input: Int): Int -> let local = input + 1; local.\n";
    let before = checked(&format!("module sample.\n{body}"));
    let after = checked(&format!("module sample.\npub other(): Int -> 0.\n{body}"));
    let id = |value: &Value| {
        value["core"]["binding_identities"]["bindings"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["name"] == "local")
            .unwrap()["id"]
            .clone()
    };
    assert_eq!(id(&before), id(&after));
}

#[test]
fn same_arity_overloads_retain_distinct_declaration_origins() {
    let value = checked(
        "module sample.\npub value(input: Int): Int -> input.\npub value(input: Float): Float -> input.\n",
    );
    let functions = value["core"]["functions"].as_array().unwrap();
    let declarations = value["source_declarations"].as_array().unwrap();
    assert_eq!(functions.len(), 2);
    for declaration in declarations {
        let Some(payload) = declaration.get("payload") else {
            continue;
        };
        if payload["kind"] != "function" {
            continue;
        }
        let matches = functions
            .iter()
            .filter(|function| function["source"]["declaration_span"] == declaration["span"])
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "each source overload has one exact Core origin"
        );
        assert_eq!(matches[0]["source"]["module"], "sample");
        assert_eq!(matches[0]["source"]["function"], "value");
        assert_eq!(
            matches[0]["params"][0]["ty"],
            payload["params"][0]["annotation"]["text"]
        );
    }
}

#[test]
fn source_set_checks_imports_with_the_shared_interface_graph() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("library.terli"),
        "module library.\npub value(input: Int): Int.\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("main.terl"),
        "module main.\nimport library.{value}.\npub run(): Int -> value(1).\n",
    )
    .unwrap();
    let (bundle, status) = export_sources(root.path(), &CliState::default()).unwrap();
    assert_eq!(status, ExitCode::SUCCESS);
    assert_eq!(bundle["units"].as_array().unwrap().len(), 2);
    assert_eq!(bundle["units"][0]["export"]["module_name"], "library");
    assert_eq!(bundle["units"][0]["export"]["status"], "interface_only");
    assert_eq!(bundle["units"][1]["export"]["status"], "checked");
    assert_eq!(bundle["units"][1]["export"]["core"]["module"], "main");
}

#[test]
fn a_rejected_source_prevents_successful_bundle_status() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("broken.terl"),
        "module broken.\npub value(): Bool -> 1.\n",
    )
    .unwrap();
    std::fs::write(
        root.path().join("valid.terl"),
        "module valid.\npub value(): Int -> 1.\n",
    )
    .unwrap();
    let (bundle, status) = export_sources(root.path(), &CliState::default()).unwrap();
    assert_ne!(status, ExitCode::SUCCESS);
    assert_eq!(bundle["status"], "invalid");
    assert_eq!(bundle["units"][0]["export"]["status"], "invalid");
    assert!(bundle["units"][0]["export"]["core"].is_null());
    assert_eq!(bundle["units"][1]["export"]["status"], "checked");
}
