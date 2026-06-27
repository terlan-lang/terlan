use super::*;

/// Verifies Wasm manifest reservations are preserved in package metadata.
///
/// Inputs:
/// - A parsed `wasm-browser` project manifest with bridge and capabilities.
///
/// Output:
/// - Test assertion only; serialized package metadata must contain a stable
///   `wasm` target object and omit `wasi`.
///
/// Transformation:
/// - Projects manifest target metadata through the Rust serde model without
///   invoking Wasm emission.
#[test]
fn wasm_artifact_metadata_projects_wasm_browser_target() {
    let manifest = project_manifest::parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"wasm-browser\"\n\n[target.wasm]\nprofile = \"browser\"\nexports = [\"app.TodoList\", \"app.TodoStore\"]\nbridge = \"generated-js\"\ncapabilities = [\"browser.console\", \"browser.scope\", \"browser.fetch\"]\nvalidation_engine = \"browser-playwright\"\n",
        std::path::Path::new("terlan.toml"),
    )
    .expect("manifest should parse");

    let metadata = build_package_metadata(std::path::Path::new("."), &manifest, &[]);
    let value = serde_json::to_value(metadata).expect("serialize package metadata");

    assert_eq!(value["artifact"], "wasm-browser");
    assert_eq!(value["wasm"]["profile"], "browser");
    assert_eq!(value["wasm"]["exports"][0], "app.TodoList");
    assert_eq!(value["wasm"]["exports"][1], "app.TodoStore");
    assert_eq!(value["wasm"]["bridge"], "generated-js");
    assert_eq!(value["wasm"]["capabilities"][2], "browser.fetch");
    assert_eq!(value["wasm"]["validation_engine"], "browser-playwright");
    assert!(value.get("wasi").is_none());
    assert!(value.get("executable").is_none());
}

/// Verifies WASI manifest reservations are preserved in package metadata.
///
/// Inputs:
/// - A parsed `wasi-cli` project manifest with world and capabilities.
///
/// Output:
/// - Test assertion only; serialized package metadata must contain a stable
///   `wasi` target object and omit `wasm`.
///
/// Transformation:
/// - Projects manifest target metadata through the Rust serde model without
///   invoking WASI component emission.
#[test]
fn wasm_artifact_metadata_projects_wasi_cli_target() {
    let manifest = project_manifest::parse_project_manifest(
        "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nartifact = \"wasi-cli\"\n\n[target.wasi]\nprofile = \"cli\"\nworld = \"wasi:cli/command\"\ncapabilities = [\"stdio\", \"args\", \"env\", \"filesystem.read\"]\nvalidation_engine = \"wasmtime\"\n",
        std::path::Path::new("terlan.toml"),
    )
    .expect("manifest should parse");

    let metadata = build_package_metadata(std::path::Path::new("."), &manifest, &[]);
    let value = serde_json::to_value(metadata).expect("serialize package metadata");

    assert_eq!(value["artifact"], "wasi-cli");
    assert_eq!(value["wasi"]["profile"], "cli");
    assert_eq!(value["wasi"]["world"], "wasi:cli/command");
    assert_eq!(value["wasi"]["capabilities"][0], "stdio");
    assert_eq!(value["wasi"]["capabilities"][3], "filesystem.read");
    assert_eq!(value["wasi"]["validation_engine"], "wasmtime");
    assert!(value.get("wasm").is_none());
    assert!(value.get("executable").is_none());
}
