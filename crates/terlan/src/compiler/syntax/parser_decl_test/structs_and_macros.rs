use crate::terlan_syntax::parse_tree::{Decl, Expr};
use crate::terlan_syntax::{parse_interface_module, parse_module};

#[test]
fn rejects_ambiguous_constructor_clause_shapes() {
    let duplicate_exact = r#"
module bad.

pub constructor Pair {
    (A: Int): Pair ->
        make(A);

    (B: Binary): Pair ->
        make(B)
}.
"#;

    let err = parse_module(duplicate_exact).expect_err("ambiguous exact arity");
    assert_eq!(err.message, "constructor has ambiguous arity clauses");

    let overlapping_defaults = r#"
module bad.

pub constructor Range {
    (Start: Int, End: Int = 10): Range ->
        make(Start, End);

    (Start: Int): Range ->
        make(Start, 10)
}.
"#;

    let err = parse_module(overlapping_defaults).expect_err("ambiguous default arity");
    assert_eq!(err.message, "constructor has ambiguous arity clauses");

    let duplicate_varargs = r#"
module bad.

pub constructor Items[T] {
    (...Items: T): Items[T] ->
        Items;

    (First: T, ...Rest: T): Items[T] ->
        Rest
}.
"#;

    let err = parse_module(duplicate_varargs).expect_err("ambiguous varargs");
    assert_eq!(err.message, "constructor has ambiguous varargs clauses");
}

#[test]
fn rejects_misplaced_module_doc_comments() {
    let source = r#"
module misplaced_docs.

//! Late module docs.
pub id(X: Int): Int ->
    X.
"#;

    let err = parse_module(source).expect_err("reject misplaced module docs");
    assert_eq!(
        err.message,
        "module doc comments (`//!`) must appear before the module declaration"
    );

    let interface_source = r#"
module misplaced_interface_docs.

//! Late module docs.
pub id(X: Int): Int.
"#;

    let interface_err =
        parse_interface_module(interface_source).expect_err("reject misplaced interface docs");
    assert_eq!(
        interface_err.message,
        "module doc comments (`//!`) must appear before the module declaration"
    );
}

#[test]
fn rejects_misplaced_module_doc_blocks() {
    let source = r#"
module misplaced_doc_block.

/**
 * Late module docs.
 *
 * @module misplaced_doc_block
 */
pub id(x: Int): Int ->
    x.
"#;

    let err = parse_module(source).expect_err("reject misplaced module doc block");
    assert_eq!(
            err.message,
            "module documentation blocks (`/** ... @module ... */`) must appear before the module declaration"
        );

    let interface_source = r#"
module misplaced_interface_doc_block.

/**
 * Late module docs.
 *
 * @module misplaced_interface_doc_block
 */
pub id(x: Int): Int.
"#;

    let interface_err =
        parse_interface_module(interface_source).expect_err("reject misplaced interface doc block");
    assert_eq!(
            interface_err.message,
            "module documentation blocks (`/** ... @module ... */`) must appear before the module declaration"
        );
}

#[test]
fn parses_struct_field_doc_comments() {
    let source = r#"
module users.

/// A user account.
pub struct User {
    /// Stable internal ID.
    id: Int,

    /// Display name.
    name: Text
}.
"#;

    let module = parse_module(source).expect("parse struct docs");
    match &module.declarations[0] {
        Decl::Struct(struct_decl) => {
            assert_eq!(struct_decl.docs, vec!["A user account."]);
            assert_eq!(struct_decl.fields[0].docs, vec!["Stable internal ID."]);
            assert_eq!(struct_decl.fields[1].docs, vec!["Display name."]);
        }
        _ => panic!("expected documented struct"),
    }
}

/// Verifies `#field` syntax in struct declarations.
///
/// Inputs:
/// - A public struct with one public field and one private field.
///
/// Output:
/// - Test passes when the parser stores the clean field name and privacy
///   flag separately.
///
/// Transformation:
/// - Parses source-level private field spelling into canonical field
///   metadata for later typechecking and interface emission.
#[test]
fn parses_private_struct_field_declarations() {
    let source = r#"
module users.

pub struct User {
    id: Int,
    #email: String
}.
"#;

    let module = parse_module(source).expect("parse private struct fields");
    match &module.declarations[0] {
        Decl::Struct(struct_decl) => {
            assert_eq!(struct_decl.fields[0].name, "id");
            assert!(!struct_decl.fields[0].is_private);
            assert_eq!(struct_decl.fields[1].name, "email");
            assert!(struct_decl.fields[1].is_private);
        }
        _ => panic!("expected struct declaration"),
    }
}

#[test]
fn parses_public_macro_declaration() {
    let source = r#"
module mathx.

pub macro unless(X: Expr, Y: Expr): Expr ->
    quote X.
"#;

    let tokens = crate::terlan_syntax::lexer::lex(source).unwrap();
    for token in tokens {
        println!("{:?} {:?} {:?}", token.kind, token.text, token.span());
    }

    let module = parse_module(source).expect("parse");
    assert_eq!(module.name, "mathx");
    assert_eq!(module.declarations.len(), 1);
    match &module.declarations[0] {
        Decl::Function(function) => assert!(function.is_macro),
        _ => panic!("expected function declaration"),
    }
}

#[test]
fn parses_public_trait_as_decl() {
    let source = r#"
module trait_demo.

/// Show trait docs.
pub trait Show[A] {
    show(Value: A): Text.
}.
"#;

    let module = parse_module(source).expect("parse");
    assert_eq!(module.declarations.len(), 1);
    match &module.declarations[0] {
        Decl::Trait(trait_decl) => {
            assert!(trait_decl.is_public);
            assert_eq!(trait_decl.name, "Show");
            assert_eq!(trait_decl.params[0], "A");
            assert_eq!(trait_decl.docs, vec!["Show trait docs."]);
        }
        _ => panic!("expected trait declaration"),
    }
}

#[test]
fn parses_raw_block_declaration_without_trailing_dot() {
    let source = r#"
module native_meta.

target vm with native_boundary.

native core module ArrayNative {
    #[native(normal)]
    length[T](A: Array[T]): Int.
}
"#;

    let module = parse_module(source).expect("parse");
    assert_eq!(module.declarations.len(), 2);
    match &module.declarations[1] {
        Decl::Raw(raw) => {
            assert_eq!(raw.kind, "native");
            assert!(raw.text.contains("ArrayNative"));
        }
        _ => panic!("expected raw native declaration"),
    }
}

#[test]
fn parses_public_struct_declaration() {
    let source = r#"
module users.

pub struct User {
    id: Int,
    name: Text,
    email: Text = Atom["none"]
}.
"#;

    let module = parse_module(source).expect("parse");
    assert_eq!(module.declarations.len(), 1);
    match &module.declarations[0] {
        Decl::Struct(struct_decl) => {
            assert!(struct_decl.is_public);
            assert_eq!(struct_decl.name, "User");
            assert_eq!(struct_decl.fields.len(), 3);
            assert_eq!(struct_decl.fields[0].name, "id");
            assert_eq!(struct_decl.fields[1].name, "name");
            assert_eq!(struct_decl.fields[2].name, "email");
            match &struct_decl.fields[2].default {
                Some(default) => match default {
                    Expr::AtomLiteral(atom) => assert_eq!(atom, "none"),
                    _ => panic!("expected atom default expression"),
                },
                None => panic!("expected default expression"),
            }
        }
        _ => panic!("expected struct declaration"),
    }
}

#[test]
fn parses_struct_includes_clause() {
    let source = r#"
module users.

pub struct User includes Person, Audited {
    id: Int
}.
"#;

    let module = parse_module(source).expect("parse struct includes");
    match &module.declarations[0] {
        Decl::Struct(struct_decl) => {
            assert_eq!(
                struct_decl.includes,
                vec!["Person".to_string(), "Audited".to_string()]
            );
        }
        _ => panic!("expected struct declaration"),
    }
}

#[test]
fn rejects_legacy_struct_derives_clause() {
    let source = r#"
module users.

pub struct User derives Person {
    id: Int
}.
"#;

    let err = parse_module(source).expect_err("reject legacy derives clause");
    assert_eq!(err.message, "expected LBrace");
}

#[test]
fn parses_trait_as_trait_decl() {
    let source = r#"
module traits.

pub trait Show {}.
"#;

    let module = parse_module(source).expect("parse");
    assert_eq!(module.declarations.len(), 1);
    match &module.declarations[0] {
        Decl::Trait(trait_decl) => {
            assert_eq!(trait_decl.name, "Show");
            assert!(trait_decl.params.is_empty());
        }
        _ => panic!("expected trait declaration"),
    }
}

#[test]
fn parses_trait_decl_extends() {
    let source = r#"
module traits.

pub trait Monoid[A] extends Semigroup[A], Eq[A] {
    combine(X: A, Y: A): A.
}.
"#;

    let module = parse_module(source).expect("parse");
    assert_eq!(module.declarations.len(), 1);
    match &module.declarations[0] {
        Decl::Trait(trait_decl) => {
            assert_eq!(trait_decl.name, "Monoid");
            assert_eq!(trait_decl.params, vec!["A"]);
            assert_eq!(trait_decl.super_traits, vec!["Semigroup[A]", "Eq[A]"]);
        }
        _ => panic!("expected trait declaration"),
    }
}

#[test]
fn parses_function_declaration_with_angle_generic_bounds() {
    let source = r#"
module bounds_demo.

pub debug<A: Eq + Show>(X: A, Y: A): Text ->
    case Eq.equal(X, Y) {
        true -> Show.render(X);
        false -> "neq"
    }.
"#;

    let module = parse_module(source).expect("parse generic bounds function");
    let function = match &module.declarations[0] {
        Decl::Function(function) => function,
        _ => panic!("expected function declaration"),
    };
    assert_eq!(function.name, "debug");
    assert_eq!(function.params.len(), 2);
    assert_eq!(function.params[0].annotation.text, "A");
    assert_eq!(function.params[1].annotation.text, "A");
}

#[test]
fn parses_trait_method_with_angle_generic_bounds() {
    let source = r#"
module bounds_trait.

pub trait Logger[A] {
    debug<A: Eq + Show>(Value: A): Text.
}.
"#;

    let module = parse_module(source).expect("parse trait method bounds");
    let trait_decl = match &module.declarations[0] {
        Decl::Trait(trait_decl) => trait_decl,
        _ => panic!("expected trait declaration"),
    };
    let method = &trait_decl.methods[0];
    assert_eq!(method.name, "debug");
    assert_eq!(method.params.len(), 1);
    assert_eq!(method.params[0].annotation.text, "A");
}
