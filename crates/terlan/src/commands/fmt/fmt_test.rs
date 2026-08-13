use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use super::{is_generated_do_not_edit, parse_source, run};
use crate::support::test_fs::{temp_dir, write_file};
use crate::terlan_syntax::migrate_repeated_let_source;

/// Verifies `terlc fmt` is the parser-backed migration path for 0.0.6 let
/// binding sequences.
#[test]
fn fmt_migrates_implicit_repeated_let_bindings() {
    let formatted = parse_source(
        "sample.terl",
        r#"
module sample.

pub total(price: Int, tax: Int): Int ->
    let {whole, fraction} = {price, tax};
    subtotal = whole + fraction;
    values = [subtotal];
    values[0] = subtotal;
    values[0].
"#,
    )
    .expect("legacy let sequence should migrate");

    assert!(formatted.contains("let {whole, fraction} = {price, tax};"));
    assert!(formatted.contains("let subtotal = whole + fraction;"));
    assert!(formatted.contains("let values = [subtotal];"));
    assert!(formatted.contains("values[0] = subtotal;"));
    assert!(!formatted.contains("let values[0]"));
    crate::terlan_syntax::parse_module_as_syntax_output(&formatted)
        .expect("migrated output should be canonical");
}

/// Verifies migration does not bless comma-grouped local declarations.
#[test]
fn fmt_rejects_comma_grouped_let_bindings() {
    let error = parse_source(
        "sample.terl",
        r#"
module sample.

pub total(price: Int, tax: Int): Int ->
    let subtotal = price, total = subtotal + tax; total.
"#,
    )
    .expect_err("comma-grouped lets must remain invalid");

    assert_eq!(
        error,
        "local bindings must be separated by `; let`, not commas"
    );
}

/// Verifies the mechanical migration preserves comments and source layout.
#[test]
fn repeated_let_migration_preserves_comments_and_assignment_boundaries() {
    let source = r#"
module sample.

pub value(input: Int): Int ->
    let first = input;
    // This comment must remain attached to the next binding.
    second = first + 1;
    values = [second];
    values[0] = second;
    values[0].
"#;

    let migrated = migrate_repeated_let_source(source).expect("source should migrate");
    assert!(migrated.contains("// This comment must remain attached to the next binding."));
    assert!(migrated.contains("    let second = first + 1;"));
    assert!(migrated.contains("    let values = [second];"));
    assert!(migrated.contains("    values[0] = second;"));
    assert!(!migrated.contains("let values[0]"));
}

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

/// Verifies `terlc fmt` collapses trivial constant helper bodies.
///
/// Inputs:
/// - A source module with a single-alias type and zero-argument constant
///   functions written over multiple lines.
///
/// Output:
/// - Formatted source containing one-line type aliases and constant helper
///   declarations.
///
/// Transformation:
/// - Exercises the command-level parse/format path so CLI formatting applies
///   the same compact alias/constant rule as the core formatter.
#[test]
fn fmt_collapses_trivial_constant_function_bodies() {
    let formatted = parse_source(
        "sample.terl",
        r#"
module sample.

pub type Cell =
    Int.

pub empty(): Int ->
    -1.

pub ready(): Bool ->
    true.
"#,
    )
    .expect("constant helpers should format");

    assert!(formatted.contains("pub type Cell = Int."));
    assert!(formatted.contains("pub empty(): Int -> -1."));
    assert!(formatted.contains("pub ready(): Bool -> true."));
    assert!(!formatted.contains("pub type Cell =\n      Int."));
    assert!(!formatted.contains("pub empty(): Int ->\n    -1."));
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

    assert!(formatted.contains("import type std.core.Error.Error as CoreError."));
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

#[test]
fn fmt_command_formats_nested_template_interpolation() {
    let dir = temp_dir("fmt", "nested_template_interpolation");
    let path = dir.join("Card.terl.html");
    write_file(&path, r#"<main>${  render(Map { body = "}" })  }</main>"#);

    let source = std::fs::read_to_string(&path).expect("read template source");
    let formatted = parse_source(&path.to_string_lossy(), &source).expect("format template");
    assert_eq!(formatted, r#"<main>${render(Map { body = "}" })}</main>"#);
}

/// Verifies the executable string-capture fixture is canonical CLI output.
///
/// Inputs:
/// - The checked-in long-tail fixture also executed by `terlc test`.
///
/// Output:
/// - Formatting succeeds and returns the fixture byte-for-byte.
///
/// Transformation:
/// - Makes `terlc fmt` and `terlc test` share one capture-heavy source anchor,
///   preventing formatter output from drifting from executable syntax.
#[test]
fn fmt_string_pattern_long_tail_fixture_is_canonical() {
    let source = include_str!("../../../../../tests/pattern/StringPatternLongTailTest.terl");
    let formatted = parse_source("StringPatternLongTailTest.terl", source)
        .expect("format string-pattern long-tail fixture");

    assert_eq!(formatted, source);
}

/// Verifies single-file formatting owns exactly one trailing newline.
///
/// Inputs:
/// - A canonical source module passed through the same parse/format path used
///   before the CLI prints single-file output.
///
/// Output:
/// - Formatted text ending in one newline and not in a blank line.
///
/// Transformation:
/// - Guards shell-redirection usage such as `terlc fmt file.terl > file.tmp`
///   from producing an extra blank line at EOF.
#[test]
fn fmt_source_output_has_no_blank_line_at_eof() {
    let formatted = parse_source(
        "sample.terl",
        r#"
module sample.

pub value(input: Int): Int -> input.
"#,
    )
    .expect("format source module");

    assert!(formatted.ends_with('\n'));
    assert!(!formatted.ends_with("\n\n"));
}

/// Verifies checked-in std tests stay canonical under `terlc fmt`.
///
/// Inputs:
/// - Every human-owned `*Test.terl` file under the workspace `std` directory;
///   generated do-not-edit bindings remain generator-owned.
///
/// Output:
/// - Test assertion listing non-canonical files, capped to keep diagnostics
///   readable.
///
/// Transformation:
/// - Runs the same parser and formatter used by `terlc fmt` and compares the
///   emitted text to the checked-in file contents.
#[test]
fn fmt_keeps_std_test_sources_canonical() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let std_root = workspace_root.join("std");
    let mut paths = Vec::new();
    collect_std_test_paths(&std_root, &mut paths).expect("collect std test paths");
    paths.sort();

    assert!(
        !paths.is_empty(),
        "expected std test files under {}",
        std_root.display()
    );

    let mut failures = Vec::new();
    for path in paths {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        if is_generated_do_not_edit(&source) {
            continue;
        }
        let relative_path = path.strip_prefix(workspace_root).unwrap_or(&path);
        let formatted = parse_source(&relative_path.to_string_lossy(), &source)
            .unwrap_or_else(|err| panic!("failed to format {}: {err}", path.display()));
        if formatted != source {
            failures.push(relative_path.display().to_string());
            if failures.len() >= 20 {
                break;
            }
        }
    }

    assert!(
        failures.is_empty(),
        "std test files must be terlc fmt clean:\n{}",
        failures.join("\n")
    );
}

/// Verifies checked-in std core sources stay canonical under `terlc fmt`.
///
/// Inputs:
/// - Every non-test `.terl` file under the workspace `std/core` directory.
///
/// Output:
/// - Test assertion listing non-canonical files, capped to keep diagnostics
///   readable.
///
/// Transformation:
/// - Runs the same parser and formatter used by `terlc fmt` and compares the
///   emitted text to the checked-in file contents so import grouping does not
///   regress in core std modules.
#[test]
fn fmt_keeps_std_core_sources_canonical() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let core_root = workspace_root.join("std/core");
    let mut paths = Vec::new();
    collect_std_core_source_paths(&core_root, &mut paths).expect("collect std core source paths");
    paths.sort();

    assert!(
        !paths.is_empty(),
        "expected std core source files under {}",
        core_root.display()
    );

    let mut failures = Vec::new();
    for path in paths {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        let relative_path = path.strip_prefix(workspace_root).unwrap_or(&path);
        let formatted = parse_source(&relative_path.to_string_lossy(), &source)
            .unwrap_or_else(|err| panic!("failed to format {}: {err}", path.display()));
        if formatted != source {
            failures.push(relative_path.display().to_string());
            if failures.len() >= 20 {
                break;
            }
        }
    }

    assert!(
        failures.is_empty(),
        "std core source files must be terlc fmt clean:\n{}",
        failures.join("\n")
    );
}

/// Verifies checked-in std collection sources stay canonical under `terlc fmt`.
///
/// Inputs:
/// - Every non-test `.terl` file under the workspace `std/collections`
///   directory.
///
/// Output:
/// - Test assertion listing non-canonical files, capped to keep diagnostics
///   readable.
///
/// Transformation:
/// - Runs the same parser and formatter used by `terlc fmt` and compares the
///   emitted text to the checked-in file contents so pipe canonicalization
///   remains enforced for collection traversal modules.
#[test]
fn fmt_keeps_std_collection_sources_canonical() {
    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let collection_root = workspace_root.join("std/collections");
    let mut paths = Vec::new();
    collect_std_non_test_source_paths(&collection_root, &mut paths)
        .expect("collect std collection source paths");
    paths.sort();

    assert!(
        !paths.is_empty(),
        "expected std collection source files under {}",
        collection_root.display()
    );

    let mut failures = Vec::new();
    for path in paths {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        let relative_path = path.strip_prefix(workspace_root).unwrap_or(&path);
        let formatted = parse_source(&relative_path.to_string_lossy(), &source)
            .unwrap_or_else(|err| panic!("failed to format {}: {err}", path.display()));
        if formatted != source {
            failures.push(relative_path.display().to_string());
            if failures.len() >= 20 {
                break;
            }
        }
    }

    assert!(
        failures.is_empty(),
        "std collection source files must be terlc fmt clean:\n{}",
        failures.join("\n")
    );
}

fn collect_std_test_paths(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir)
        .map_err(|err| format!("failed to read directory {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| {
            format!(
                "failed to read directory entry under {}: {err}",
                dir.display()
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_std_test_paths(&path, paths)?;
        } else if path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .is_some_and(|file_name| file_name.ends_with("Test.terl"))
        {
            paths.push(path);
        }
    }
    Ok(())
}

fn collect_std_core_source_paths(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    collect_std_non_test_source_paths(dir, paths)
}

fn collect_std_non_test_source_paths(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir)
        .map_err(|err| format!("failed to read directory {}: {err}", dir.display()))?
    {
        let entry = entry.map_err(|err| {
            format!(
                "failed to read directory entry under {}: {err}",
                dir.display()
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_std_non_test_source_paths(&path, paths)?;
        } else if path
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .is_some_and(|file_name| {
                file_name.ends_with(".terl") && !file_name.ends_with("Test.terl")
            })
        {
            paths.push(path);
        }
    }
    Ok(())
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

/// Verifies explicit single-file write mode formats only after parsing succeeds.
#[test]
fn fmt_write_formats_one_file_in_place() {
    let dir = temp_dir("fmt", "write_single_file");
    let path = dir.join("Sample.terl");
    write_file(
        &path,
        "module sample.\npub value( input : Int ): Int -> input.\n",
    );

    assert_eq!(
        run(&["--write".to_owned(), path.display().to_string()]),
        ExitCode::SUCCESS
    );
    assert_eq!(
        fs::read_to_string(path).expect("read formatted source"),
        "module sample.\n\npub value(input: Int): Int ->\n    input.\n"
    );
}

/// Verifies explicit write mode leaves malformed source byte-for-byte intact.
#[test]
fn fmt_write_rejects_malformed_file_without_truncation() {
    let dir = temp_dir("fmt", "write_malformed_source");
    let path = dir.join("Broken.terl");
    let source = "module broken.\npub value(: Int): Int -> 1.\n";
    write_file(&path, source);

    assert_eq!(
        run(&["--write".to_owned(), path.display().to_string()]),
        ExitCode::from(1)
    );
    assert_eq!(
        fs::read_to_string(path).expect("read rejected source"),
        source
    );
}

/// Verifies the command wrapper formats directories in place.
///
/// Inputs:
/// - A temporary directory containing nested `.terl` and `.terli` files plus a
///   non-Terlan file.
///
/// Output:
/// - Success exit code, rewritten Terlan files, and untouched non-Terlan file.
///
/// Transformation:
/// - Walks directory inputs recursively, applies extension-specific parser and
///   formatter behavior, and writes formatted output back to each source path.
#[test]
fn fmt_command_formats_directory_sources_in_place() {
    let dir = temp_dir("fmt", "directory_sources");
    let source_path = dir.join("src").join("Sample.terl");
    let interface_path = dir.join("src").join("Sample.terli");
    let ignored_path = dir.join("src").join("README.md");
    let template_path = dir.join("src").join("Card.terl.html");
    write_file(
        &source_path,
        r#"
module sample.

pub value(input: Int): Int -> input.
"#,
    );
    write_file(
        &interface_path,
        r#"
module sample.
export value/1.
"#,
    );
    write_file(&ignored_path, "# leave me alone\n");
    write_file(&template_path, "<p>${  title  }</p>\n");

    assert_eq!(run(&[dir.display().to_string()]), ExitCode::SUCCESS);

    let source = std::fs::read_to_string(&source_path).expect("read formatted source");
    let interface = std::fs::read_to_string(&interface_path).expect("read formatted interface");
    let ignored = std::fs::read_to_string(&ignored_path).expect("read ignored file");
    let template = std::fs::read_to_string(&template_path).expect("read formatted template");
    assert!(source.contains("module sample."));
    assert!(source.contains("pub value(input: Int): Int ->"));
    assert!(interface.contains("export value/1."));
    assert_eq!(ignored, "# leave me alone\n");
    assert_eq!(template, "<p>${title}</p>\n");
}

/// Verifies check mode accepts canonical input without changing it.
#[test]
fn fmt_check_accepts_canonical_file_without_mutation() {
    let dir = temp_dir("fmt", "check_canonical_file");
    let path = dir.join("Sample.terl");
    let source = "module sample.\n\npub value(input: Int): Int ->\n    input.\n";
    write_file(&path, source);

    assert_eq!(
        run(&["--check".to_owned(), path.display().to_string()]),
        ExitCode::SUCCESS
    );
    assert_eq!(
        fs::read_to_string(path).expect("read checked source"),
        source
    );
}

/// Verifies check mode reports formatting drift without changing the source.
#[test]
fn fmt_check_rejects_noncanonical_file_without_mutation() {
    let dir = temp_dir("fmt", "check_noncanonical_file");
    let path = dir.join("Sample.terl");
    let source = "module sample.\npub value( input : Int ): Int -> input.\n";
    write_file(&path, source);

    assert_eq!(
        run(&["--check".to_owned(), path.display().to_string()]),
        ExitCode::from(1)
    );
    assert_eq!(
        fs::read_to_string(path).expect("read checked source"),
        source
    );
}

/// Verifies directory check mode is non-mutating and fails on any drift.
#[test]
fn fmt_check_rejects_noncanonical_directory_without_mutation() {
    let dir = temp_dir("fmt", "check_noncanonical_directory");
    let canonical_path = dir.join("Canonical.terl");
    let drifted_path = dir.join("nested").join("Drifted.terl");
    let canonical = "module canonical.\n\npub value(input: Int): Int ->\n    input.\n";
    let drifted = "module drifted.\npub value( input : Int ): Int -> input.\n";
    write_file(&canonical_path, canonical);
    write_file(&drifted_path, drifted);

    assert_eq!(
        run(&["--check".to_owned(), dir.display().to_string()]),
        ExitCode::from(1)
    );
    assert_eq!(
        fs::read_to_string(canonical_path).expect("read canonical source"),
        canonical
    );
    assert_eq!(
        fs::read_to_string(drifted_path).expect("read drifted source"),
        drifted
    );
}

/// Verifies recursive formatting respects generator-owned do-not-edit files.
#[test]
fn fmt_directory_skips_generated_do_not_edit_sources() {
    let dir = temp_dir("fmt", "generated_do_not_edit");
    let path = dir.join("Generated.terl");
    let source = r#"/**
 * @generated true
 * @do-not-edit true
 */
module generated.
pub value( input : Int ): Int -> input.
"#;
    write_file(&path, source);

    assert_eq!(
        run(&["--check".to_owned(), dir.display().to_string()]),
        ExitCode::SUCCESS
    );
    assert_eq!(run(&[dir.display().to_string()]), ExitCode::SUCCESS);
    assert_eq!(
        fs::read_to_string(path).expect("read generated source"),
        source
    );
}

/// Verifies directory formatting fails when any contained Terlan source fails
/// to parse.
///
/// Inputs:
/// - A directory containing one malformed `.terl` file.
///
/// Output:
/// - Failure exit code `1`.
///
/// Transformation:
/// - Exercises the same parse failure path as single-file formatting after
///   recursive directory discovery.
#[test]
fn fmt_command_rejects_directory_with_malformed_source() {
    let dir = temp_dir("fmt", "directory_malformed_source");
    let path = dir.join("Broken.terl");
    write_file(&path, "module broken.\npub value(: Int): Int -> 1.\n");

    assert_eq!(run(&[dir.display().to_string()]), ExitCode::from(1));
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

/// Verifies script formatting preserves source mode and never prints a hidden entrypoint.
#[test]
fn fmt_formats_headerless_script_with_shebang_idempotently() {
    let source = "#!/usr/bin/env terlc\nimport std.io.Console.{println}.\nlet value=40+2; assert_equal(value, 42); println(\"ok\").\n";

    let formatted = parse_source("scripts/Smoke.terls", source).expect("format script");

    assert!(formatted.starts_with("#!/usr/bin/env terlc\n"));
    assert!(formatted.contains("import std.io.Console.{println}."));
    assert!(formatted.contains("value = 40 + 2;"));
    assert!(formatted.contains("assert_equal(value, 42);"));
    assert!(formatted.contains("println(\"ok\")."));
    assert!(!formatted.contains("__script_"));
    assert!(!formatted.contains("module "));
    assert!(!formatted.contains("main("));
    assert_eq!(
        parse_source("scripts/Smoke.terls", &formatted).expect("format script twice"),
        formatted
    );
}
