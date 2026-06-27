use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::terlan_syntax::parse_module_as_syntax_output;

use super::{collect_syntax_markdown_frontend_inputs, collect_syntax_template_frontend_inputs};

/// Builds a unique temporary directory for artifact tests.
///
/// Inputs:
/// - `name`: stable test-name prefix.
///
/// Output:
/// - A path under the system temporary directory.
///
/// Transformation:
/// - Combines the current process id and wall-clock nanoseconds so tests
///   can run repeatedly without reusing previous fixture directories.
fn temp_artifact_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("terlan-{name}-{}-{unique}", std::process::id()))
}

/// Proves the A0.50 template frontend collector preserves normalized input.
///
/// Inputs:
/// - No external input; the test creates a temporary Terlan source file and
///   sibling template file.
///
/// Output:
/// - Test assertion result.
///
/// Transformation:
/// - Parses syntax output, resolves and parses the declared template file,
///   then checks preserved declaration metadata and parsed HTML metadata.
#[test]
fn collect_syntax_template_frontend_inputs_preserves_normalized_template_metadata() {
    let dir = temp_artifact_dir("template-frontend-input");
    fs::create_dir_all(&dir).expect("create temp artifact dir");
    let source_path = dir.join("page_test.terl");
    let template_path = dir.join("page.terl.html");
    fs::write(
        &source_path,
        r#"module page_test.

template Page from "page.terl.html" {
  title: String
}.
"#,
    )
    .expect("write source fixture");
    fs::write(
        &template_path,
        r#"@template { params = { title: String } }

<template tag="page-view"><h1>${title}</h1></template>"#,
    )
    .expect("write template fixture");

    let source = fs::read_to_string(&source_path).expect("read source fixture");
    let module = parse_module_as_syntax_output(&source).expect("parse source fixture");
    let collected = collect_syntax_template_frontend_inputs(&module, &source_path);

    assert!(collected.errors.is_empty(), "{:?}", collected.errors);
    assert_eq!(collected.inputs.len(), 1);
    let input = &collected.inputs[0];
    assert_eq!(input.name, "Page");
    assert_eq!(input.source_path, "page.terl.html");
    assert_eq!(input.resolved_path, template_path);
    assert_eq!(input.props.len(), 1);
    assert_eq!(input.props[0].name, "title");
    assert_eq!(input.props[0].annotation.text, "String");
    assert!(input.metadata.params_declared);
    assert_eq!(input.metadata.params.len(), 1);
    assert_eq!(input.metadata.params[0].name, "title");
    assert_eq!(input.metadata.params[0].type_text, "String");
    assert!(input.span.end > input.span.start);
    assert_eq!(input.parsed.tag_name.as_deref(), Some("page"));

    fs::remove_dir_all(&dir).expect("remove temp artifact dir");
}

/// Rejects template header metadata that drifts from the source declaration.
///
/// Inputs:
/// - A source template declaration with one prop.
/// - A sibling template file whose `@template.params` declares a different
///   prop name.
///
/// Output:
/// - Test passes when the frontend collector reports a deterministic metadata
///   mismatch.
///
/// Transformation:
/// - Proves annotation-backed template signatures are validated before
///   generated template functions consume them.
#[test]
fn collect_syntax_template_frontend_inputs_rejects_template_metadata_mismatch() {
    let dir = temp_artifact_dir("template-frontend-metadata-mismatch");
    fs::create_dir_all(&dir).expect("create temp artifact dir");
    let source_path = dir.join("page_test.terl");
    let template_path = dir.join("page.terl.html");
    fs::write(
        &source_path,
        r#"module page_test.

template Page from "page.terl.html" {
  title: String
}.
"#,
    )
    .expect("write source fixture");
    fs::write(
        &template_path,
        r#"@template { params = { heading: String } }

<template tag="page-view"><h1>${heading}</h1></template>"#,
    )
    .expect("write template fixture");

    let source = fs::read_to_string(&source_path).expect("read source fixture");
    let module = parse_module_as_syntax_output(&source).expect("parse source fixture");
    let collected = collect_syntax_template_frontend_inputs(&module, &source_path);

    assert!(collected.inputs.is_empty());
    assert_eq!(collected.errors.len(), 1);
    assert_eq!(
        collected.errors[0].message,
        "template `Page` metadata param 1 is `heading`, but source declaration prop is `title`"
    );

    fs::remove_dir_all(&dir).expect("remove temp artifact dir");
}

/// Proves Markdown frontend collection preserves source metadata.
///
/// Inputs:
/// - A temporary Terlan source file importing one `.terl.md` Markdown document
///   with an `@page` header.
///
/// Output:
/// - Test passes when alias, source path, resolved path, page metadata, and
///   parsed Markdown body are all preserved.
///
/// Transformation:
/// - Exercises the metadata-preserving Markdown collector used by static-site
///   route discovery.
#[test]
fn collect_syntax_markdown_frontend_inputs_preserves_page_metadata() {
    let dir = temp_artifact_dir("markdown-frontend-input");
    fs::create_dir_all(&dir).expect("create temp artifact dir");
    let source_path = dir.join("site_test.terl");
    let markdown_path = dir.join("install.terl.md");
    fs::write(
        &source_path,
        r#"module site_test.

import markdown "install.terl.md" as Install.
"#,
    )
    .expect("write source fixture");
    fs::write(
        &markdown_path,
        r#"@page { title = "Install", route = "/install", layout = "docs" }

# Install
"#,
    )
    .expect("write markdown fixture");

    let source = fs::read_to_string(&source_path).expect("read source fixture");
    let module = parse_module_as_syntax_output(&source).expect("parse source fixture");
    let collected = collect_syntax_markdown_frontend_inputs(&module, &source_path)
        .expect("collect markdown frontend inputs");

    assert_eq!(collected.len(), 1);
    let input = &collected[0];
    assert_eq!(input.alias, "Install");
    assert_eq!(input.source_path, "install.terl.md");
    assert_eq!(input.resolved_path, markdown_path);
    assert_eq!(input.metadata.title.as_deref(), Some("Install"));
    assert_eq!(input.metadata.route.as_deref(), Some("/install"));
    assert_eq!(input.metadata.layout.as_deref(), Some("docs"));
    assert_eq!(input.document.raw_source, "# Install\n");

    fs::remove_dir_all(&dir).expect("remove temp artifact dir");
}
