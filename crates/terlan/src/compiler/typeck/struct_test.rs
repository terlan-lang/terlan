use super::test_support::*;
use super::*;
use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn interface_first_local_include_chains_preserve_ancestor_fields_and_metadata() {
    let declarations = [
        "pub struct Base { code: Int = 7, #secret: String }.",
        "pub struct Middle includes Base { middle: Bool }.",
        "pub struct Leaf includes Middle { leaf: String }.",
    ];
    for order in [[0, 1, 2], [2, 1, 0], [1, 2, 0]] {
        let source = format!(
            "module inheritance. {}",
            order.map(|index| declarations[index]).join(" ")
        );
        let module = parse_module_as_syntax_output(&source).unwrap();
        let base = module
            .declarations
            .iter()
            .find_map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Struct { name, fields, .. } if name == "Base" => {
                    Some(fields.clone())
                }
                _ => None,
            })
            .unwrap();
        let resolved = crate::terlan_hir::resolve_syntax_module_output(&module).module;
        let (expanded, diagnostics) = expand_syntax_includes(module, &resolved);
        assert!(diagnostics.is_empty(), "{source}: {diagnostics:?}");
        let leaf = expanded
            .declarations
            .iter()
            .find_map(|declaration| match &declaration.payload {
                SyntaxDeclarationPayload::Struct { name, fields, .. } if name == "Leaf" => {
                    Some(fields)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            leaf.iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            ["code", "secret", "middle", "leaf"]
        );
        assert_eq!(&leaf[..base.len()], &base);
    }
}

#[test]
fn interface_first_local_include_cycles_and_transitive_conflicts_are_atomic() {
    for (declarations, message) in [
        ("pub struct A includes B { a: Int }. pub struct B includes A { b: Int }.", "struct include cycle: A -> B -> A"),
        ("pub struct A includes B { a: Int }. pub struct B includes C { b: Int }. pub struct C includes A { c: Int }.", "struct include cycle: A -> B -> C -> A"),
        ("pub struct Leaf includes Middle { code: String }. pub struct Middle includes Base { middle: Bool }. pub struct Base { code: Int }.", "field `code` conflicts"),
        ("pub struct Leaf includes Left, Right { leaf: Int }. pub struct Left includes Base { left: Int }. pub struct Right includes Base { right: Int }. pub struct Base { code: Int }.", "field `code` conflicts"),
    ] {
        let source = format!("module inheritance. pub struct Root {{ id: Int }}. pub struct Valid includes Root {{ value: Int }}. {declarations}");
        let module = parse_module_as_syntax_output(&source).unwrap();
        let resolved = crate::terlan_hir::resolve_syntax_module_output(&module).module;
        let (expanded, diagnostics) = expand_syntax_includes(module.clone(), &resolved);
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(message)), "{source}: {diagnostics:?}");
        assert_eq!(expanded, module, "invalid inclusion must not partially expand other declarations");
    }
}

#[test]
fn expands_syntax_includes_no_ops_without_struct_includes() {
    let module = parse_module_as_syntax_output(
        "\
module syntax_include_expansion_ok.\n\
pub struct User {\n\
    id: Int\n\
}.\n",
    )
    .expect("parse syntax-output include expansion fixture");
    let resolved = crate::terlan_hir::resolve_syntax_module_output(&module).module;

    let (expanded, diagnostics) = expand_syntax_includes(module.clone(), &resolved);

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    assert_eq!(
        expanded, module,
        "non-including modules must pass through unchanged"
    );
}

#[test]
fn expands_syntax_includes_copies_local_parent_struct_fields() {
    let module = parse_module_as_syntax_output(
        "\
module syntax_include_expansion_fields.\n\
pub struct Error {\n\
    code: Atom,\n\
    message: String\n\
}.\n\
\n\
pub struct FileError includes Error {\n\
    path: String\n\
}.\n",
    )
    .expect("parse syntax-output include expansion fixture");
    let resolved = crate::terlan_hir::resolve_syntax_module_output(&module).module;

    let (expanded, diagnostics) = expand_syntax_includes(module, &resolved);

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
    let file_error_fields = expanded
        .declarations
        .iter()
        .find_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Struct { name, fields, .. } if name == "FileError" => Some(
                fields
                    .iter()
                    .map(|field| field.name.as_str())
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .expect("expanded FileError struct");
    assert_eq!(file_error_fields, vec!["code", "message", "path"]);
}

#[test]
fn syntax_output_checks_opaque_constructor_returns_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_opaque_returns.\n\
pub opaque type UserId = Int.\n\
pub user_id(value: Int): UserId ->\n\
    UserId(value).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn explicit_constructor_may_return_option() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        r#"
module checked.OptionalProfile.

import std.core.Option.{Option, Some}.

pub struct Profile {
    name: String
}.

pub constructor Profile {
    (name: String): Option[Profile] -> Some(Profile { name: name })
}.
"#,
        "std/core/Option.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn explicit_constructor_may_return_result() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        r#"
module checked.ResultProfile.

import std.core.Result.{Result, Ok}.

pub struct Profile {
    name: String
}.

pub constructor Profile {
    (name: String): Result[Profile, String] -> Ok(Profile { name: name })
}.
"#,
        "std/core/Result.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn syntax_output_checks_struct_includes_on_formal_path() {
    let valid_diagnostics = check_syntax_output(
        "\
module struct_includes_ok.\n\
pub struct Error {\n\
    code: Atom,\n\
    message: String\n\
}.\n\
\n\
pub struct FileError includes Error {\n\
    path: String\n\
}.\n\
",
    );
    assert!(
        valid_diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        valid_diagnostics
    );

    let unknown_diagnostics = check_syntax_output(
        "\
module struct_includes_unknown.\n\
pub struct User includes NoSuch {\n\
    id: Int\n\
}.\n\
",
    );
    assert!(
        unknown_diagnostics
            .iter()
            .any(|diag| diag.message.contains("unknown included struct `NoSuch`")),
        "diagnostics: {:?}",
        unknown_diagnostics
    );

    let trait_instance_diagnostics = check_syntax_output(
        "\
module struct_includes_trait_instance.\n\
pub trait Show[A] {\n\
    show(value: A): Binary.\n\
}.\n\
\n\
pub struct User includes Show[User] {\n\
    id: Int\n\
}.\n\
",
    );
    assert!(
        trait_instance_diagnostics.iter().any(|diag| diag
            .message
            .contains("must be a struct name, not a trait or generic instance")),
        "diagnostics: {:?}",
        trait_instance_diagnostics
    );

    let duplicate_diagnostics = check_syntax_output(
        "\
module struct_includes_duplicate.\n\
pub struct Error {\n\
    code: Atom\n\
}.\n\
\n\
pub struct User includes Error, Error {\n\
    id: Int\n\
}.\n\
",
    );
    assert!(
        duplicate_diagnostics
            .iter()
            .any(|diag| diag.message.contains("duplicate included struct `Error`")),
        "diagnostics: {:?}",
        duplicate_diagnostics
    );
}
