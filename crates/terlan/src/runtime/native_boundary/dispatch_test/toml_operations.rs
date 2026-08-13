use super::*;

#[test]
fn toml_parse_dispatches_into_the_shared_json_representation() {
    let Some(NativeBoundaryValue::Json(root)) = dispatch_ok(
        "std.data.toml.parse",
        &[NativeBoundaryValue::Text(
            "[package]\nname = \"terlan\"\nfeatures = [\"aot\"]\n".to_string(),
        )],
    ) else {
        return;
    };

    let package = json::get(&root, "package").expect("package table");
    let name = json::get(&package, "name").expect("package name");
    assert_eq!(json::as_string(&name), Ok("terlan".to_string()));
    let features = json::get(&package, "features").expect("package features");
    assert_eq!(json::length(&features), Ok(1));
}

#[test]
fn malformed_toml_preserves_the_typed_boundary_error() {
    let error = dispatch(
        "std.data.toml.parse",
        &[NativeBoundaryValue::Text(
            "[package\nname = \"terlan\"".to_string(),
        )],
    )
    .expect_err("reject malformed TOML");

    assert_eq!(error.code(), "toml.parse");
    assert!(!error.message().is_empty());
    assert_eq!(error.offset(), 0);
}
