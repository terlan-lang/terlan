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

#[test]
fn parse_preserves_scalar_types_and_array_tables() {
    let value = parse(
        "enabled = true\nport = 8080\nratio = 1.5\n\
         [[routes]]\npath = '/health'\nmethods = ['GET', 'HEAD']\n",
    )
    .expect("parse TOML values");
    assert_eq!(
        value.as_serde(),
        &serde_json::json!({
            "enabled": true,
            "port": 8080,
            "ratio": 1.5,
            "routes": [{"path": "/health", "methods": ["GET", "HEAD"]}]
        })
    );
}

#[test]
fn parse_rejects_duplicate_keys() {
    let error = parse("port = 8080\nport = 9090\n").expect_err("reject duplicate keys");
    assert_eq!(error.code(), "toml.parse");
    assert!(!error.message().is_empty());
}
