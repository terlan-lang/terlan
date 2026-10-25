use super::*;

#[test]
fn local_binding_types_preserve_the_original_typescript_module() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("metadata")).unwrap();
    std::fs::write(directory.path().join("Host.typi"), "module app.Host.\n").unwrap();
    let source = directory.path().join("Host.d.ts");
    std::fs::write(&source, "export interface Host<Value> { view?: Value; }\n").unwrap();
    let mut registry = Registry::default();
    registry.merge(r#"{"outputs":[{"module":"app.Host","summary":"Host.typi","typescript_type":{"source":"Host.d.ts","name":"Host"},"members":[]}]}"#, Some(&directory.path().join("metadata/manifest.json"))).unwrap();
    let declaration = &registry.typescript_types["app.Host"];
    assert_eq!(declaration.source, source.canonicalize().unwrap());
    assert_eq!(declaration.name, "Host");
}

#[test]
fn generated_package_keeps_its_typescript_member_contract() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(directory.path().join("metadata")).unwrap();
    std::fs::write(directory.path().join("Host.typi"), "module app.Host.\n").unwrap();
    let manifest = r#"{"outputs":[{"module":"app.Host","summary":"Host.typi","members":[{"name":"listen","js_name":"onEvent","kind":"method","arity":2,"return_type":"Unit","parameters":[{"optional":false,"type":"(String) -> Unit"}]}]}]}"#;
    let mut registry = Registry::default();
    registry
        .merge(
            manifest,
            Some(&directory.path().join("metadata/bindings.json")),
        )
        .unwrap();
    let member = &registry.modules["app.Host"][0];
    assert_eq!(member.js_name, "onEvent");
    assert_eq!(member.parameters[0].ty, "(String) -> Unit");
    assert_eq!(
        registry.summaries["app.Host"],
        directory.path().join("Host.typi")
    );
}

#[test]
fn rejects_inconsistent_host_call_metadata() {
    let mut registry = Registry::default();
    let error = registry.merge(r#"{"outputs":[{"module":"app.Host","members":[{"name":"read","js_name":"read","kind":"method","arity":1,"return_type":"String","parameters":[{"optional":false,"type":"String"}]}]}]}"#, None).unwrap_err();
    assert!(error.contains("invalid JavaScript binding arity"));
    assert!(registry
        .merge(
            r#"{"outputs":[{"module":"app.Host"},{"module":"app.Host"}]}"#,
            None
        )
        .unwrap_err()
        .contains("duplicate"));
}

#[test]
fn record_operations_validate_field_names_and_argument_counts() {
    let mut registry = Registry::default();
    for member in [
        serde_json::json!({"name":"construct", "js_name":"", "kind":"record", "arity":1, "return_type":"Host", "js_names":["value"], "parameters":[{"optional":false,"type":"Bool"}]}),
        serde_json::json!({"name":"set_value", "js_name":"value", "kind":"setter", "arity":2, "return_type":"Unit", "js_names":["value"], "parameters":[{"optional":false,"type":"Bool"}]}),
    ] {
        let source =
            serde_json::json!({"outputs":[{"module":"app.Host", "members":[member]}]}).to_string();
        registry.merge(&source, None).unwrap();
    }
    for member in [
        serde_json::json!({"name":"construct", "js_name":"", "kind":"record", "arity":1, "return_type":"Host", "js_names":[], "parameters":[{"optional":false,"type":"Bool"}]}),
        serde_json::json!({"name":"construct", "js_name":"", "kind":"record", "arity":2, "return_type":"Host", "js_names":["value","value"], "parameters":[{"optional":false,"type":"Bool"},{"optional":false,"type":"Bool"}]}),
        serde_json::json!({"name":"set_value", "js_name":"other", "kind":"setter", "arity":2, "return_type":"Unit", "js_names":["value"], "parameters":[{"optional":false,"type":"Bool"}]}),
    ] {
        let source =
            serde_json::json!({"outputs":[{"module":"app.Host", "members":[member]}]}).to_string();
        assert!(registry.merge(&source, None).is_err());
    }
}
