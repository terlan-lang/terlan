use super::parse;

#[test]
fn parse_preserves_nested_manifest_values() {
    let value = parse("[package]\nname = \"terlan\"\nfeatures = [\"aot\"]\n").expect("parse TOML");
    let package = value.as_serde().get("package").expect("package table");
    assert_eq!(
        package.get("name").and_then(|value| value.as_str()),
        Some("terlan")
    );
    assert_eq!(
        package
            .get("features")
            .and_then(|value| value.as_array())
            .map(Vec::len),
        Some(1)
    );
}

#[test]
fn parse_rejects_malformed_toml() {
    let error = parse("[package\nname = \"terlan\"").expect_err("reject malformed TOML");
    assert_eq!(error.code(), "toml.parse");
    assert!(!error.message().is_empty());
}
