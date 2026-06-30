use std::process::ExitCode;

use super::{parse_source, run};
use crate::support::test_fs::{temp_dir, write_file};

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

/// Verifies the command wrapper rejects malformed argument counts.
///
/// Inputs:
/// - Empty and overfull command-local argument lists.
///
/// Output:
/// - Usage exit code `2`.
///
/// Transformation:
/// - Exercises the public `fmt` command boundary before any filesystem or
///   parser work is attempted.
#[test]
fn fmt_command_rejects_missing_or_extra_path_argument() {
    assert_eq!(run(&[]), ExitCode::from(2));
    assert_eq!(
        run(&["one.terl".to_owned(), "two.terl".to_owned()]),
        ExitCode::from(2)
    );
}

/// Verifies the command wrapper reports file-read failures.
///
/// Inputs:
/// - A unique path that was not created.
///
/// Output:
/// - Failure exit code `1`.
///
/// Transformation:
/// - Routes through `support::read_file` and stops before syntax parsing.
#[test]
fn fmt_command_rejects_missing_input_file() {
    let dir = temp_dir("fmt", "missing_input_file");
    let missing = dir.join("missing.terl");

    assert_eq!(run(&[missing.display().to_string()]), ExitCode::from(1));
}

/// Verifies the command wrapper accepts source-module files.
///
/// Inputs:
/// - A temporary `.terl` file containing canonical source text.
///
/// Output:
/// - Success exit code.
///
/// Transformation:
/// - Reads from disk, selects source-module parsing by extension, and prints
///   the formatter result.
#[test]
fn fmt_command_formats_source_module_file() {
    let dir = temp_dir("fmt", "source_module_file");
    let path = dir.join("Sample.terl");
    write_file(
        &path,
        r#"
module sample.

pub value(input: Int): Int -> input.
"#,
    );

    assert_eq!(run(&[path.display().to_string()]), ExitCode::SUCCESS);
}

/// Verifies the command wrapper accepts interface summary files.
///
/// Inputs:
/// - A temporary `.terli` file containing an export summary.
///
/// Output:
/// - Success exit code.
///
/// Transformation:
/// - Reads from disk, selects interface parsing by extension, and prints the
///   formatter result.
#[test]
fn fmt_command_formats_interface_file() {
    let dir = temp_dir("fmt", "interface_file");
    let path = dir.join("Sample.terli");
    write_file(
        &path,
        r#"
module sample.
export value/1.
"#,
    );

    assert_eq!(run(&[path.display().to_string()]), ExitCode::SUCCESS);
}

/// Verifies parse diagnostics become command failures.
///
/// Inputs:
/// - A temporary `.terl` file with malformed source.
///
/// Output:
/// - Failure exit code `1`.
///
/// Transformation:
/// - Reads the file successfully and fails through the formal parser route.
#[test]
fn fmt_command_rejects_malformed_source_file() {
    let dir = temp_dir("fmt", "malformed_source_file");
    let path = dir.join("Broken.terl");
    write_file(&path, "module broken.\npub value(: Int): Int -> 1.\n");

    assert_eq!(run(&[path.display().to_string()]), ExitCode::from(1));
}
