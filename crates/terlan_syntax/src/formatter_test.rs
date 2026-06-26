use super::*;
use crate::{parse_interface_module, parse_module};

/// Verifies block documentation is emitted with canonical marker spacing.
///
/// Inputs:
/// - A source module containing a TypeDoc-style block where body lines are
///   written as `*Text` instead of `* Text`.
///
/// Output:
/// - Formatted source preserving the doc block with one space after each body
///   marker.
///
/// Transformation:
/// - Parses documentation through the lexer-normalized doc metadata and renders
///   it back as canonical `/** ... */` formatter output.
#[test]
fn formatter_normalizes_doc_block_marker_spacing() {
    let output = format_source_module(
        r#"
module doc_spacing_fmt.

/**
 *Core boolean conformance helpers.
 *@param value input value.
 *@returns canonical bool.
 */
pub value(value: Bool): Bool ->
    value.
"#,
    )
    .expect("source with doc block should format");

    assert!(output.contains(" * Core boolean conformance helpers."));
    assert!(output.contains(" * @param value input value."));
    assert!(output.contains(" * @returns canonical bool."));
    assert!(!output.contains("*Core boolean"));
    assert!(!output.contains("*@param"));
    assert!(!output.contains("*@returns"));
}

/// Verifies wildcard imports use the braced selector form after formatting.
///
/// Inputs:
/// - Path-style wildcard import syntax.
///
/// Output:
/// - Canonical import source using `.{*}.`.
///
/// Transformation:
/// - Parses the compatibility import spelling and renders the stable wildcard
///   import selector form so the declaration terminator stays visually clear.
#[test]
fn formatter_canonicalizes_wildcard_imports() {
    let output = format_source_module(
        r#"
module wildcard_import_fmt.

import test.Other.*.

pub main(): Int -> 1.
"#,
    )
    .expect("format wildcard import");

    assert!(output.contains("import test.Other.{*}."));
    assert!(!output.contains("import test.Other.*."));
}

/// Verifies formatter output organizes imports alphabetically.
///
/// Inputs:
/// - A source module with imports in non-alphabetical order.
///
/// Output:
/// - Formatted source whose import declarations are sorted by canonical import
///   text before ordinary declarations.
///
/// Transformation:
/// - Parses and formats the module, then compares the emitted import line order
///   after wildcard spelling has been canonicalized.
#[test]
fn formatter_sorts_imports_alphabetically() {
    let output = format_source_module(
        r#"
module sorted_import_fmt.

import std.io.Console.{println}.
import app.z.Zed.
import app.alpha.Alpha.
import app.middle.Tools.{*}.

pub main(): Int -> 1.
"#,
    )
    .expect("format sorted imports");

    let import_lines = output
        .lines()
        .filter(|line| line.starts_with("import "))
        .collect::<Vec<_>>();
    let mut sorted_import_lines = import_lines.clone();
    sorted_import_lines.sort();
    assert_eq!(import_lines, sorted_import_lines);
    assert_eq!(import_lines.first(), Some(&"import app.alpha. Alpha."));
    assert_eq!(
        import_lines.last(),
        Some(&"import std.io.Console. println.")
    );
}

/// Verifies source modules cannot reach formatter export-list normalization.
///
/// Inputs:
/// - A canonical `.terl` module string containing removed source `export`
///   syntax.
///
/// Output:
/// - Parse diagnostic from the source parser.
///
/// Transformation:
/// - Attempts source parsing before formatting, proving formatter export
///   support is not a source-module escape hatch.
#[test]
fn formatter_source_parser_rejects_export_declarations() {
    let error = parse_module(
        r#"
module formatter_source_export.

export ghost/1.
"#,
    )
    .expect_err("source parser must reject export declarations before formatting");

    assert!(error
        .message
        .contains("source export declarations are not part of canonical Terlan"));
}

/// Verifies interface export summaries can still round-trip through the shared formatter.
///
/// Inputs:
/// - A `.terli` interface string containing an export summary.
///
/// Output:
/// - Formatted interface text preserving `export ghost/1.`.
///
/// Transformation:
/// - Parses with the interface parser and formats the resulting parse tree,
///   using the shared declaration formatter's interface-only export branch.
#[test]
fn formatter_preserves_interface_export_summaries() {
    let module = parse_interface_module(
        r#"
module formatter_interface_export.

export ghost/1.
"#,
    )
    .expect("interface export summaries remain valid");

    let output = format_module(&module);

    assert!(output.contains("export ghost/1."));
}

/// Verifies HTML blocks are formatted with nested shape and stable attributes.
///
/// Inputs:
/// - A module containing compact inline HTML with attributes out of canonical
///   order.
///
/// Output:
/// - Formatted module text containing indented HTML with sorted attributes.
///
/// Transformation:
/// - Parses the module and runs the shared source formatter over the parse
///   tree.
#[test]
fn formats_html_blocks_with_nested_shape_and_sorted_attrs() {
    let module = parse_module(
        r#"
module html_fmt.

pub view(Name: Text): Html[none] ->
    html { <main id="home" class="page"><h1>{Name}</h1><input value={Name} name="email" /></main> }.
"#,
    )
    .expect("parse module");

    let output = format_module(&module);
    assert!(output.contains(
        "html {\n        <main class=\"page\" id=\"home\">\n            <h1>\n                {Name}\n            </h1>\n            <input name=\"email\" value={Name} />\n        </main>\n    }"
    ));
}

/// Verifies file imports round-trip through formatter output.
///
/// Inputs:
/// - A module containing a `file` asset import.
///
/// Output:
/// - Formatted module text preserving the import path and alias.
///
/// Transformation:
/// - Parses the module and formats the parse tree.
#[test]
fn formats_file_imports() {
    let module = parse_module(
        r#"
module file_import_fmt.

import file "./templates/user_card.terl.html" as UserCard.
"#,
    )
    .expect("parse module");

    let output = format_module(&module);
    assert!(output.contains(r#"import file "./templates/user_card.terl.html" as UserCard."#));
}

/// Verifies CSS imports round-trip through formatter output.
///
/// Inputs:
/// - A module containing a `css` asset import.
///
/// Output:
/// - Formatted module text preserving the import path and alias.
///
/// Transformation:
/// - Parses the module and formats the parse tree.
#[test]
fn formats_css_imports() {
    let module = parse_module(
        r#"
module css_import_fmt.

import css "./styles/page.css" as PageCss.
"#,
    )
    .expect("parse module");

    let output = format_module(&module);
    assert!(output.contains(r#"import css "./styles/page.css" as PageCss."#));
}

/// Verifies Markdown imports round-trip through formatter output.
///
/// Inputs:
/// - A module containing a `markdown` asset import.
///
/// Output:
/// - Formatted module text preserving the import path and alias.
///
/// Transformation:
/// - Parses the module and formats the parse tree.
#[test]
fn formats_markdown_imports() {
    let module = parse_module(
        r#"
module markdown_import_fmt.

import markdown "./posts/hello.md" as HelloPost.
"#,
    )
    .expect("parse module");

    let output = format_module(&module);
    assert!(output.contains(r#"import markdown "./posts/hello.md" as HelloPost."#));
}

/// Verifies template declarations round-trip through formatter output.
///
/// Inputs:
/// - A module containing a template declaration with props.
///
/// Output:
/// - Formatted template declaration text with stable indentation.
///
/// Transformation:
/// - Parses the module and formats the parse tree.
#[test]
fn formats_template_declarations() {
    let module = parse_module(
        r#"
module template_fmt.

template Page from "./templates/page.terl.html" {
    title: Text,
    user: User
}.
"#,
    )
    .expect("parse module");

    let output = format_module(&module);
    assert!(output.contains(
        "template Page from \"./templates/page.terl.html\" {\n    title: Text,\n    user: User\n}."
    ));
}

/// Verifies template instantiation expressions round-trip through formatter output.
///
/// Inputs:
/// - A module containing a fielded template instantiation expression.
///
/// Output:
/// - Formatted template expression with stable field text.
///
/// Transformation:
/// - Parses the module and formats the parse tree.
#[test]
fn formats_template_instantiation_exprs() {
    let module = parse_module(
        r#"
module template_instantiation_fmt.

pub view(Title: Text, User: User): Html[none] ->
    Page{ title = Title, user = User }.
"#,
    )
    .expect("parse module");

    let output = format_module(&module);
    assert!(output.contains("Page {title = Title, user = User}."));
}
