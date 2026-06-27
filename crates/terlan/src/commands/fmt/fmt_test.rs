use super::parse_source;

/// Verifies that `terlc fmt` keeps canonical source modules on `pub`
/// visibility instead of normalizing removed export-list syntax.
///
/// Inputs:
/// - A `.terl` path and source containing a source-mode `export` declaration.
///
/// Output:
/// - A parse error containing the canonical source-export diagnostic.
///
/// Transformation:
/// - Routes the source through the same formal syntax-output parser and parse tree
///   formatter preparation used by the CLI command.
#[test]
fn fmt_rejects_source_export_declarations() {
    let error = parse_source(
        "sample.terl",
        r#"
module sample.
export add/1.
add(x: Int): Int -> x.
"#,
    )
    .expect_err("source export declarations must be rejected before formatting");

    assert!(error.contains("source export declarations are not part of canonical Terlan"));
}

/// Verifies that `terlc fmt` still treats `.terli` export summaries as
/// interface metadata rather than source module visibility.
///
/// Inputs:
/// - A `.terli` path and interface source containing an export summary.
///
/// Output:
/// - Formatted interface text preserving the export summary.
///
/// Transformation:
/// - Selects interface parsing by extension, validates the formal
///   syntax-output path, then formats the parse tree interface module.
#[test]
fn fmt_preserves_interface_export_summaries() {
    let formatted = parse_source(
        "sample.terli",
        r#"
module sample.
export add/1.
"#,
    )
    .expect("interface export summaries remain valid formatter input");

    assert!(formatted.contains("export add/1."));
}

/// Verifies `terlc fmt` canonicalizes noisy default-export type imports.
///
/// Inputs:
/// - A source module importing `std.core.Error.Error`, where the final path
///   segment repeats the imported type name.
///
/// Output:
/// - Formatted source using `import type std.core.Error.`.
///
/// Transformation:
/// - Parses through the formal syntax-output path, formats through the
///   source formatter, and applies the default-export import shorthand only
///   when the selected type has no alias.
#[test]
fn fmt_collapses_redundant_default_type_import() {
    let formatted = parse_source(
        "sample.terl",
        r#"
module sample.

import type std.core.Error.Error.

pub value(error: Error): Error -> error.
"#,
    )
    .expect("redundant default type import should format");

    assert!(formatted.contains("import type std.core.Error."));
    assert!(!formatted.contains("import type std.core.Error.Error."));
}

/// Verifies `terlc fmt` normalizes TypeDoc block marker spacing.
///
/// Inputs:
/// - A source module containing documentation lines written as `*Text`.
///
/// Output:
/// - Formatted source containing `* Text`.
///
/// Transformation:
/// - Routes source through the formal syntax-output parser and source
///   formatter used by the CLI so file formatting and stdlib policy checks
///   enforce the same documentation shape.
#[test]
fn fmt_normalizes_doc_block_marker_spacing() {
    let formatted = parse_source(
        "sample.terl",
        r#"
/**
 *Module docs.
 */
module sample.

/**
 *Returns the input.
 *
 *Input: one integer.
 *Output: the same integer.
 *Transformation: identity.
 */
pub value(input: Int): Int -> input.
"#,
    )
    .expect("doc marker spacing should format");

    assert!(formatted.contains(" * Module docs."));
    assert!(formatted.contains(" * Returns the input."));
    assert!(formatted.contains(" * Input: one integer."));
    assert!(!formatted.contains("*Returns"));
}

/// Verifies `terlc fmt` keeps aliased default-export type imports explicit.
///
/// Inputs:
/// - A source module importing `std.core.Error.Error as CoreError`.
///
/// Output:
/// - Formatted source preserving the selected import and alias.
///
/// Transformation:
/// - Guards against collapsing aliased imports because the shorthand cannot
///   represent a caller-selected local name.
#[test]
fn fmt_preserves_aliased_default_type_import() {
    let formatted = parse_source(
        "sample.terl",
        r#"
module sample.

import type std.core.Error.Error as CoreError.

pub value(error: CoreError): CoreError -> error.
"#,
    )
    .expect("aliased default type import should format");

    assert!(formatted.contains("import type std.core.Error. Error as CoreError."));
}
