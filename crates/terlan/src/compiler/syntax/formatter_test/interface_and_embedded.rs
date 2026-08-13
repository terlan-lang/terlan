use super::super::format_module;
use crate::terlan_syntax::{parse_interface_module, parse_module};

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
pub(super) fn formatter_preserves_interface_export_summaries() {
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
pub(super) fn formats_html_blocks_with_nested_shape_and_sorted_attrs() {
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

/// Preserved HTML fallback text must remain stable across formatter passes.
#[test]
pub(super) fn formats_html_comprehension_blocks_idempotently() {
    let source = r#"
module html_comprehension_fmt.

pub view(Users: List[User]): Html[none] ->
    html {
        <ul>
            {[html {
                <li>{User}</li>
            } | User <- Users]}
        </ul>
    }.
"#;
    let once = format_module(&parse_module(source).expect("parse HTML comprehension"));
    let twice = format_module(&parse_module(&once).expect("parse formatted comprehension"));

    assert_eq!(twice, once);
}

/// Raw declarations whose parser span includes the terminator stay parseable.
#[test]
pub(super) fn formats_raw_declaration_terminators_idempotently() {
    let source = r#"
module raw_declaration_fmt.

target erlang with native_boundary.

native core module NativeArray { # [ native ( normal ) ] length [ T ] ( A : Array [ T ] ) : Int . }.
"#;
    let once = format_module(&parse_module(source).expect("parse raw declarations"));
    let twice = format_module(&parse_module(&once).expect("parse formatted raw declarations"));

    assert_eq!(twice, once);
    assert!(once.contains("target erlang with native_boundary."));
    assert!(!once.contains("native_boundary.."));
    assert!(!once.contains("}.."));
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
pub(super) fn formats_file_imports() {
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
pub(super) fn formats_css_imports() {
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
pub(super) fn formats_markdown_imports() {
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
pub(super) fn formats_template_declarations() {
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
        "template Page from \"./templates/page.terl.html\" {\n    title: Text,\n    user: User,\n}."
    ));
}

/// Verifies nominal keyed construction expressions round-trip through formatter output.
///
/// Inputs:
/// - A module containing a fielded nominal construction expression.
///
/// Output:
/// - Formatted template expression with stable field text.
///
/// Transformation:
/// - Parses the module and formats the parse tree.
#[test]
pub(super) fn formats_nominal_keyed_construction_exprs() {
    let module = parse_module(
        r#"
module template_instantiation_fmt.

pub view(Title: Text, User: User): Html[none] ->
    Page { title: Title, user: User }.
"#,
    )
    .expect("parse module");

    let output = format_module(&module);
    assert!(output.contains("Page {title: Title, user: User}."));
}
