use super::*;

#[test]
fn declaration_generation_pins_inputs_and_preserves_optional_generic_fields() {
    let root = tempfile::tempdir().unwrap();
    let source = "export interface Entry<View> { roomId: string; view?: View; nullable: View | null; lookup(): View | undefined; }\n";
    std::fs::write(root.path().join("Host.d.ts"), source).unwrap();
    let options = Options {
        input: "Host.d.ts".into(),
        namespace: "app.host".into(),
        output: root.path().join("generated"),
    };
    generate(root.path(), &options).unwrap();
    let generated = std::fs::read_to_string(options.output.join("app/host/Entry.terl")).unwrap();
    assert!(
        generated.contains("view(): Option[View]")
            || generated.contains("view(): std.core.Option.Option[View]"),
        "{generated}"
    );
    let provenance: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(options.output.join("typescript-inputs.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(provenance["inputs"][0]["path"], "Host.d.ts");
    assert_eq!(
        provenance["inputs"][0]["sha256"],
        crate::support::sha256sum_file(&root.path().join("Host.d.ts")).unwrap()
    );
    let bindings: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(options.output.join("std/js/manifests/std_js_bindings.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(bindings["outputs"][0]["module"], "app.host.Entry");
    assert_eq!(
        bindings["outputs"][0]["typescript_type"]["source"],
        "Host.d.ts"
    );
    assert_eq!(bindings["outputs"][0]["typescript_type"]["name"], "Entry");
    let members = bindings["outputs"][0]["members"].as_array().unwrap();
    for (name, expected) in [("view", true), ("nullable", false), ("lookup", true)] {
        let member = members
            .iter()
            .find(|member| member["name"] == name)
            .unwrap();
        assert_eq!(member["undefined_only"], expected, "{member}");
    }
    assert!(generate(root.path(), &options)
        .unwrap_err()
        .contains("non-empty"));
}

#[test]
fn direct_declaration_arguments_reject_missing_duplicate_and_unknown_options() {
    let arguments = [
        "--input",
        "Host.d.ts",
        "--namespace",
        "app.host",
        "--out",
        "generated",
    ]
    .map(str::to_string);
    assert_eq!(parse(&arguments).unwrap().namespace, "app.host");
    for args in [
        vec![],
        vec!["--input"],
        vec!["--namespace", "--out"],
        vec!["--wat", "value"],
        vec!["--out", "a", "--out", "b"],
    ] {
        assert!(parse(&args.into_iter().map(str::to_string).collect::<Vec<_>>()).is_err());
    }
}

#[test]
fn data_record_helpers_preserve_mutability_and_refuse_incomplete_construction() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Host.d.ts"), r#"
export interface Mutable<Value> { readonly id: string; type: string; view?: Value; nullable: Value | null; readonly WAITING: boolean; }
export interface PartialRecord { okay: boolean; unsupported: unknown; }
export interface Base { base: boolean; }
export interface Derived extends Base { own: boolean; }
export interface Literal { status: 'ready'; }
export interface Constrained<Value extends string> { value: Value; }
export interface Unresolved { title: string; nested: Missing; }
export interface Collisions { construct: string; value: boolean; set_value: string; }
"#).unwrap();
    let options = Options {
        input: "Host.d.ts".into(),
        namespace: "app.host".into(),
        output: root.path().join("generated"),
    };
    generate(root.path(), &options).unwrap();
    let bindings: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(options.output.join("std/js/manifests/std_js_bindings.json"))
            .unwrap(),
    )
    .unwrap();
    let outputs = bindings["outputs"].as_array().unwrap();
    let members = |name: &str| {
        outputs
            .iter()
            .find(|module| module["module"] == format!("app.host.{name}"))
            .unwrap()["members"]
            .as_array()
            .unwrap()
    };
    let mutable = members("Mutable");
    let construct = mutable
        .iter()
        .find(|member| member["kind"] == "record")
        .unwrap();
    assert_eq!(
        construct["js_names"],
        serde_json::json!(["id", "type", "view", "nullable", "WAITING"])
    );
    assert_eq!(construct["parameters"][2]["undefined_only"], true);
    assert_eq!(construct["parameters"][3]["undefined_only"], false);
    assert!(!mutable.iter().any(|member| member["kind"] == "setter"
        && matches!(member["js_name"].as_str(), Some("id" | "WAITING"))));
    assert!(mutable.iter().any(|member| member["name"] == "set_view"));
    let source = std::fs::read_to_string(options.output.join("app/host/Mutable.terl")).unwrap();
    assert!(source.contains("pub construct[Value]"), "{source}");
    assert!(source.contains("set_view(value:"), "{source}");
    for name in [
        "PartialRecord",
        "Derived",
        "Literal",
        "Constrained",
        "Unresolved",
    ] {
        assert!(
            !members(name)
                .iter()
                .any(|member| member["kind"] == "record"),
            "unsafe constructor for {name}"
        );
    }
    assert!(!members("Literal")
        .iter()
        .any(|member| member["kind"] == "setter"));
    assert!(!members("Constrained")
        .iter()
        .any(|member| member["kind"] == "setter"));
    assert!(members("Collisions")
        .iter()
        .any(|member| member["name"] == "construct_2"));
    assert!(members("Collisions")
        .iter()
        .any(|member| member["name"] == "set_value_2" && member["js_name"] == "value"));
}
