use super::{
    mask_json_interpolations, mask_plain_interpolations, mask_toml_interpolations,
    validate_artifact_template_structure, validate_json_template_structure,
    validate_text_template_structure, validate_toml_template_structure,
    validate_yaml_template_structure,
};
use std::path::Path;

/// Dispatches HTML templates to the HTML parser.
///
/// Inputs:
/// - `.terl.html` source and path.
///
/// Output:
/// - Test passes when target-aware validation succeeds.
///
/// Transformation:
/// - Verifies the common artifact validator reuses existing HTML parsing.
#[test]
fn validate_artifact_template_structure_accepts_html_template() {
    assert!(validate_artifact_template_structure(
        "<main><h1>Hello</h1></main>",
        "templates/page.terl.html",
    )
    .is_ok());
}

/// Dispatches JSON templates to the JSON structure validator.
///
/// Inputs:
/// - `.terl.json` source and path.
///
/// Output:
/// - Test passes when target-aware validation accepts JSON interpolation.
///
/// Transformation:
/// - Verifies callers can validate JSON templates through the common entrypoint
///   rather than invoking the JSON validator directly.
#[test]
fn validate_artifact_template_structure_accepts_json_template() {
    assert!(validate_artifact_template_structure(
        r#"{"features": ${features}}"#,
        "templates/data.terl.json",
    )
    .is_ok());
}

/// Dispatches YAML templates to the YAML structure validator.
///
/// Inputs:
/// - `.terl.yaml` source and path.
///
/// Output:
/// - Test passes when target-aware validation accepts YAML interpolation.
///
/// Transformation:
/// - Verifies callers can validate YAML templates through the common entrypoint
///   rather than invoking the YAML validator directly.
#[test]
fn validate_artifact_template_structure_accepts_yaml_template() {
    assert!(validate_artifact_template_structure(
        "title: ${title}\nitems:\n  - one\n  - two\n",
        "templates/data.terl.yaml",
    )
    .is_ok());
}

/// Accepts plain text artifact templates.
///
/// Inputs:
/// - `.terl.txt` source and path.
///
/// Output:
/// - Test passes when text validation succeeds without structural parsing.
///
/// Transformation:
/// - Locks down the initial text-template rule that arbitrary text is valid
///   until expression-island typechecking is added.
#[test]
fn validate_artifact_template_structure_accepts_text_template() {
    assert!(
        validate_artifact_template_structure("Hello ${name}", "templates/message.terl.txt",)
            .is_ok()
    );
}

/// Rejects malformed text interpolation through the common validator.
///
/// Inputs:
/// - `.terl.txt` source containing an empty interpolation island.
///
/// Output:
/// - Test passes when target-aware validation returns the text interpolation
///   diagnostic.
///
/// Transformation:
/// - Verifies the common dispatcher does not let malformed text interpolation
///   pass unchecked.
#[test]
fn validate_artifact_template_structure_rejects_bad_text_interpolation() {
    let diagnostics =
        validate_artifact_template_structure("Hello ${ }", "templates/message.terl.txt")
            .expect_err("empty interpolation should fail");

    assert_eq!(diagnostics[0].message, "empty text template interpolation");
}

/// Rejects unknown artifact-template suffixes.
///
/// Inputs:
/// - Source path without a Terlan artifact-template suffix.
///
/// Output:
/// - Test passes when validation returns a suffix diagnostic.
///
/// Transformation:
/// - Keeps the common validator strict about the target contract.
#[test]
fn validate_artifact_template_structure_rejects_unknown_suffix() {
    let diagnostics = validate_artifact_template_structure("{}", "templates/data.json")
        .expect_err("unknown suffix should fail");

    assert_eq!(
        diagnostics[0].message,
        "unknown Terlan artifact-template suffix"
    );
}

/// Verifies oversized empty structured-template interpolations fail early.
///
/// Inputs:
/// - JSON, YAML, and TOML artifact templates containing large whitespace-only
///   interpolation islands.
///
/// Output:
/// - Test passes when each target returns its interpolation diagnostic instead
///   of delegating malformed masked content to the backend parser.
///
/// Transformation:
/// - Exercises the hostile asset-template boundary where a generated fixture
///   can be large while still semantically empty.
#[test]
fn adversarial_structured_templates_reject_oversized_empty_interpolations() {
    let whitespace = " ".repeat(8192);
    let json_source = ["{\"value\": ${", &whitespace, "}}"].concat();
    let yaml_source = ["value: ${", &whitespace, "}\n"].concat();
    let toml_source = ["value = ${", &whitespace, "}\n"].concat();

    for (source, path, expected) in [
        (
            json_source.as_str(),
            "templates/data.terl.json",
            "empty JSON template interpolation",
        ),
        (
            yaml_source.as_str(),
            "templates/data.terl.yaml",
            "empty YAML template interpolation",
        ),
        (
            toml_source.as_str(),
            "templates/data.terl.toml",
            "empty TOML template interpolation",
        ),
    ] {
        let diagnostics = validate_artifact_template_structure(source, path)
            .expect_err("oversized empty interpolation should fail");
        assert_eq!(diagnostics[0].message, expected);
    }
}

/// Dispatches TOML templates to the TOML structure validator.
///
/// Inputs:
/// - `.terl.toml` source and path.
///
/// Output:
/// - Test passes when target-aware validation accepts TOML interpolation.
///
/// Transformation:
/// - Verifies callers can validate TOML templates through the common entrypoint
///   rather than invoking the TOML validator directly.
#[test]
fn validate_artifact_template_structure_accepts_toml_template() {
    assert!(validate_artifact_template_structure(
        "name = ${name}\nfeatures = [\"typed\"]\n",
        "templates/config.terl.toml",
    )
    .is_ok());
}

/// Accepts static TOML templates.
///
/// Inputs:
/// - TOML document without interpolation.
///
/// Output:
/// - Test passes when structure validation succeeds.
///
/// Transformation:
/// - Verifies the validator delegates ordinary TOML to the `toml` crate.
#[test]
fn validate_toml_template_structure_accepts_static_toml() {
    assert!(validate_toml_template_structure(
        "name = \"Terlan\"\nfeatures = [\"typed\", \"target-neutral\"]\n",
        "templates/site.terl.toml",
    )
    .is_ok());
}

/// Accepts interpolation in TOML value position.
///
/// Inputs:
/// - TOML document with `${name}` where a TOML value belongs.
///
/// Output:
/// - Test passes when interpolation is masked as a TOML string value.
///
/// Transformation:
/// - Exercises structured interpolation where Terlan supplies an encoded TOML
///   value later.
#[test]
fn validate_toml_template_structure_accepts_value_interpolation() {
    assert!(validate_toml_template_structure(
        "name = ${name}\ncount = ${count}\n",
        "templates/site.terl.toml",
    )
    .is_ok());
}

/// Accepts interpolation inside TOML strings.
///
/// Inputs:
/// - TOML document with `${name}` inside a string literal.
///
/// Output:
/// - Test passes when interpolation is masked as string text.
///
/// Transformation:
/// - Keeps mixed static/interpolated strings structurally valid while later
///   typechecking owns expression compatibility.
#[test]
fn validate_toml_template_structure_accepts_string_interpolation() {
    assert!(validate_toml_template_structure(
        "message = \"Hello ${name}\"\n",
        "templates/site.terl.toml",
    )
    .is_ok());
}

/// Preserves UTF-8 text while masking TOML interpolations.
///
/// Inputs:
/// - TOML source containing non-ASCII text and a string interpolation.
///
/// Output:
/// - Test passes when the masked source keeps the original UTF-8 text.
///
/// Transformation:
/// - Exercises the private masking transformation so validation cannot corrupt
///   user-authored Unicode before delegating to the TOML parser.
#[test]
fn mask_toml_interpolations_preserves_unicode_text() {
    let masked = mask_toml_interpolations(
        "message = \"Café ${name}\"\n",
        Path::new("templates/site.terl.toml"),
    )
    .expect("mask TOML interpolation");

    assert_eq!(masked, "message = \"Café __terlan_interpolation__\"\n");
}

/// Rejects malformed TOML after interpolation masking.
///
/// Inputs:
/// - TOML document with an unterminated array.
///
/// Output:
/// - Test passes when structure validation reports a diagnostic.
///
/// Transformation:
/// - Proves validation comes from the `toml` crate instead of accepting
///   arbitrary text templates.
#[test]
fn validate_toml_template_structure_rejects_invalid_toml() {
    let diagnostics =
        validate_toml_template_structure("features = [\"typed\"\n", "templates/site.terl.toml")
            .expect_err("invalid TOML should fail");

    assert!(diagnostics[0]
        .message
        .contains("invalid TOML template structure"));
}

/// Rejects unterminated TOML interpolation islands.
///
/// Inputs:
/// - TOML document containing `${name` without a closing brace.
///
/// Output:
/// - Test passes when interpolation masking reports a diagnostic before TOML
///   parsing.
///
/// Transformation:
/// - Keeps template-expression boundary errors distinct from TOML parser
///   errors.
#[test]
fn validate_toml_template_structure_rejects_unterminated_interpolation() {
    let diagnostics =
        validate_toml_template_structure("name = ${name\n", "templates/site.terl.toml")
            .expect_err("unterminated interpolation should fail");

    assert_eq!(
        diagnostics[0].message,
        "unterminated TOML template interpolation"
    );
}

/// Rejects empty TOML interpolation islands.
///
/// Inputs:
/// - TOML document containing `${ }`.
///
/// Output:
/// - Test passes when interpolation masking reports the empty expression.
///
/// Transformation:
/// - Prevents placeholder-only templates from bypassing future expression
///   typechecking.
#[test]
fn validate_toml_template_structure_rejects_empty_interpolation() {
    let diagnostics = validate_toml_template_structure("name = ${ }\n", "templates/site.terl.toml")
        .expect_err("empty interpolation should fail");

    assert_eq!(diagnostics[0].message, "empty TOML template interpolation");
}

/// Accepts static YAML templates.
///
/// Inputs:
/// - YAML document without interpolation.
///
/// Output:
/// - Test passes when structure validation succeeds.
///
/// Transformation:
/// - Verifies the validator delegates ordinary YAML to `yaml-rust`.
#[test]
fn validate_yaml_template_structure_accepts_static_yaml() {
    assert!(validate_yaml_template_structure(
        "name: Terlan\nfeatures:\n  - typed\n  - target-neutral\n",
        "templates/site.terl.yaml",
    )
    .is_ok());
}

/// Accepts interpolation in YAML value position.
///
/// Inputs:
/// - YAML document with `${title}` where a YAML value belongs.
///
/// Output:
/// - Test passes when interpolation is masked as a YAML scalar value.
///
/// Transformation:
/// - Exercises structured interpolation where Terlan supplies an encoded YAML
///   value later.
#[test]
fn validate_yaml_template_structure_accepts_value_interpolation() {
    assert!(validate_yaml_template_structure(
        "title: ${title}\ncount: ${count}\n",
        "templates/site.terl.yaml",
    )
    .is_ok());
}

/// Preserves UTF-8 text while masking YAML interpolations.
///
/// Inputs:
/// - YAML source containing non-ASCII text and an interpolation.
///
/// Output:
/// - Test passes when the masked source keeps the original UTF-8 text.
///
/// Transformation:
/// - Exercises the shared plain-structured masking transformation used by YAML
///   so parser validation receives source-equivalent Unicode.
#[test]
fn mask_plain_interpolations_preserves_unicode_text() {
    let masked = mask_plain_interpolations(
        "title: Café ${title}\n",
        Path::new("templates/site.terl.yaml"),
        "YAML template interpolation",
        "__terlan_interpolation__",
    )
    .expect("mask YAML interpolation");

    assert_eq!(masked, "title: Café __terlan_interpolation__\n");
}

/// Rejects malformed YAML after interpolation masking.
///
/// Inputs:
/// - YAML document with an unterminated flow sequence.
///
/// Output:
/// - Test passes when structure validation reports a diagnostic.
///
/// Transformation:
/// - Proves validation comes from `yaml-rust` instead of accepting arbitrary
///   text templates.
#[test]
fn validate_yaml_template_structure_rejects_invalid_yaml() {
    let diagnostics =
        validate_yaml_template_structure("items: [one, two\n", "templates/site.terl.yaml")
            .expect_err("invalid YAML should fail");

    assert!(diagnostics[0]
        .message
        .contains("invalid YAML template structure"));
}

/// Rejects unterminated YAML interpolation islands.
///
/// Inputs:
/// - YAML document containing `${title` without a closing brace.
///
/// Output:
/// - Test passes when interpolation masking reports a diagnostic before YAML
///   parsing.
///
/// Transformation:
/// - Keeps template-expression boundary errors distinct from YAML parser
///   errors.
#[test]
fn validate_yaml_template_structure_rejects_unterminated_interpolation() {
    let diagnostics =
        validate_yaml_template_structure("title: ${title\n", "templates/site.terl.yaml")
            .expect_err("unterminated interpolation should fail");

    assert_eq!(
        diagnostics[0].message,
        "unterminated YAML template interpolation"
    );
}

/// Rejects empty YAML interpolation islands.
///
/// Inputs:
/// - YAML document containing `${ }`.
///
/// Output:
/// - Test passes when interpolation masking reports the empty expression.
///
/// Transformation:
/// - Keeps YAML interpolation validation aligned with JSON, TOML, and text
///   artifact-template targets.
#[test]
fn validate_yaml_template_structure_rejects_empty_interpolation() {
    let diagnostics = validate_yaml_template_structure("title: ${ }\n", "templates/site.terl.yaml")
        .expect_err("empty interpolation should fail");

    assert_eq!(diagnostics[0].message, "empty YAML template interpolation");
}

/// Accepts a static JSON template.
///
/// Inputs:
/// - JSON object without interpolation.
///
/// Output:
/// - Test passes when structure validation succeeds.
///
/// Transformation:
/// - Verifies the validator delegates ordinary JSON to `serde_json`.
#[test]
fn validate_json_template_structure_accepts_static_json() {
    assert!(validate_json_template_structure(
        r#"{"name":"Terlan","features":["typed","target-neutral"]}"#,
        "templates/site.terl.json",
    )
    .is_ok());
}

/// Accepts plain text with interpolation islands.
///
/// Inputs:
/// - Text template source containing `${name}`.
///
/// Output:
/// - Test passes when text validation succeeds.
///
/// Transformation:
/// - Exercises the text-specific validator without routing through the common
///   dispatcher.
#[test]
fn validate_text_template_structure_accepts_text_interpolation() {
    assert!(
        validate_text_template_structure("Hello ${name}", "templates/message.terl.txt").is_ok()
    );
}

/// Rejects unterminated text interpolation islands.
///
/// Inputs:
/// - Text template source containing `${name` without a closing brace.
///
/// Output:
/// - Test passes when text validation reports an unterminated island.
///
/// Transformation:
/// - Keeps plain text templates from carrying malformed expression islands into
///   later renderer stages.
#[test]
fn validate_text_template_structure_rejects_unterminated_interpolation() {
    let diagnostics =
        validate_text_template_structure("Hello ${name", "templates/message.terl.txt")
            .expect_err("unterminated interpolation should fail");

    assert_eq!(
        diagnostics[0].message,
        "unterminated text template interpolation"
    );
}

/// Accepts interpolation in JSON value position.
///
/// Inputs:
/// - JSON object with `${features}` where a JSON value belongs.
///
/// Output:
/// - Test passes when the interpolation is masked as a JSON value.
///
/// Transformation:
/// - Exercises structured interpolation where Terlan supplies an encoded JSON
///   value later.
#[test]
fn validate_json_template_structure_accepts_value_interpolation() {
    assert!(validate_json_template_structure(
        r#"{"features": ${features}, "count": ${count}}"#,
        "templates/site.terl.json",
    )
    .is_ok());
}

/// Accepts interpolation inside JSON strings.
///
/// Inputs:
/// - JSON object with `${name}` inside a string literal.
///
/// Output:
/// - Test passes when the interpolation is masked as string text.
///
/// Transformation:
/// - Keeps mixed static/interpolated strings structurally valid while later
///   typechecking owns expression compatibility.
#[test]
fn validate_json_template_structure_accepts_string_interpolation() {
    assert!(validate_json_template_structure(
        r#"{"message": "Hello ${name}"}"#,
        "templates/site.terl.json",
    )
    .is_ok());
}

/// Preserves UTF-8 text while masking JSON interpolations.
///
/// Inputs:
/// - JSON source containing non-ASCII text and an interpolation inside a
///   string.
///
/// Output:
/// - Test passes when the masked source keeps the original UTF-8 text.
///
/// Transformation:
/// - Exercises JSON-specific masking so string-state tracking does not corrupt
///   Unicode while replacing expression islands.
#[test]
fn mask_json_interpolations_preserves_unicode_text() {
    let masked = mask_json_interpolations(
        r#"{"message":"Café ${name}"}"#,
        Path::new("templates/site.terl.json"),
    )
    .expect("mask JSON interpolation");

    assert_eq!(masked, r#"{"message":"Café __terlan_interpolation__"}"#);
}

/// Rejects malformed JSON after interpolation masking.
///
/// Inputs:
/// - JSON object with a trailing comma.
///
/// Output:
/// - Test passes when structure validation reports a diagnostic.
///
/// Transformation:
/// - Proves validation comes from `serde_json` instead of accepting arbitrary
///   text templates.
#[test]
fn validate_json_template_structure_rejects_invalid_json() {
    let diagnostics = validate_json_template_structure(
        r#"{"features": ${features},}"#,
        "templates/site.terl.json",
    )
    .expect_err("invalid JSON should fail");

    assert!(diagnostics[0]
        .message
        .contains("invalid JSON template structure"));
}

/// Rejects unterminated interpolation islands.
///
/// Inputs:
/// - JSON object containing `${name` without a closing brace.
///
/// Output:
/// - Test passes when interpolation masking reports a diagnostic before JSON
///   parsing.
///
/// Transformation:
/// - Keeps template-expression boundary errors distinct from JSON parser
///   errors.
#[test]
fn validate_json_template_structure_rejects_unterminated_interpolation() {
    let diagnostics =
        validate_json_template_structure(r#"{"message": "${name"}"#, "templates/site.terl.json")
            .expect_err("unterminated interpolation should fail");

    assert_eq!(
        diagnostics[0].message,
        "unterminated JSON template interpolation"
    );
}

/// Rejects empty interpolation islands.
///
/// Inputs:
/// - JSON object containing `${ }`.
///
/// Output:
/// - Test passes when interpolation masking reports the empty expression.
///
/// Transformation:
/// - Prevents placeholder-only templates from bypassing future expression
///   typechecking.
#[test]
fn validate_json_template_structure_rejects_empty_interpolation() {
    let diagnostics =
        validate_json_template_structure(r#"{"message": "${ }"}"#, "templates/site.terl.json")
            .expect_err("empty interpolation should fail");

    assert_eq!(diagnostics[0].message, "empty JSON template interpolation");
}
