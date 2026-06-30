use std::collections::{HashMap, HashSet};

use crate::terlan_hir::{FunctionSignature, ModuleInterface, ParamSignature, TraitSignature};
use crate::terlan_syntax::parse_module_as_syntax_output;

use super::html::html_start_tag_at;
use super::{
    hover_component_prop_type, hover_imported_docs, hover_local_docs, hover_record_field_type,
    interface_item_docs, line_column_to_offset, parse_hover_args, qualified_import_member,
    record_access_at,
};

fn parse(source: &str) -> crate::terlan_syntax::SyntaxModuleOutput {
    parse_module_as_syntax_output(source).expect("parse hover fixture")
}

fn empty_interface(module: &str) -> ModuleInterface {
    ModuleInterface {
        module: module.to_string(),
        docs: Vec::new(),
        public_types: HashSet::new(),
        private_types: HashSet::new(),
        opaque_types: HashSet::new(),
        type_params: HashMap::new(),
        type_bodies: HashMap::new(),
        struct_fields: HashMap::new(),
        type_docs: HashMap::new(),
        traits: HashMap::new(),
        trait_conformances: Vec::new(),
        constructors: HashMap::new(),
        functions: HashMap::new(),
        function_overloads: HashMap::new(),
    }
}

#[test]
fn parse_hover_args_accepts_column_alias() {
    let args = vec![
        "src/app/Main.terl".to_string(),
        "--line".to_string(),
        "3".to_string(),
        "--col".to_string(),
        "9".to_string(),
    ];

    assert_eq!(parse_hover_args(&args), Ok(("src/app/Main.terl", 3, 9)));
}

#[test]
fn parse_hover_args_rejects_missing_or_non_positive_coordinates() {
    let missing = vec!["src/app/Main.terl".to_string()];
    assert_eq!(
        parse_hover_args(&missing),
        Err("hover requires a path, --line, and --column".to_string())
    );

    let zero = vec![
        "src/app/Main.terl".to_string(),
        "--line".to_string(),
        "0".to_string(),
        "--column".to_string(),
        "1".to_string(),
    ];
    assert_eq!(
        parse_hover_args(&zero),
        Err("hover line and column must be positive integers".to_string())
    );
}

#[test]
fn line_column_to_offset_handles_unicode_and_eof() {
    let source = "α\nabc";

    assert_eq!(line_column_to_offset(source, 1, 1), Some(0));
    assert_eq!(line_column_to_offset(source, 2, 2), Some("α\na".len()));
    assert_eq!(line_column_to_offset(source, 2, 4), Some(source.len()));
    assert_eq!(line_column_to_offset(source, 3, 1), None);
}

#[test]
fn record_access_at_detects_struct_field_span() {
    let source = "#User.display_name";
    let offset = source.find("display").expect("field offset");

    assert_eq!(
        record_access_at(source, offset),
        Some(("User".to_string(), "display_name".to_string()))
    );
    assert_eq!(record_access_at("User.display_name", offset), None);
}

#[test]
fn qualified_import_member_reads_dotted_module_prefix() {
    let source = "std.core.String.trim";
    let offset = source.find("trim").expect("member offset");

    assert_eq!(
        qualified_import_member(source, offset),
        Some(("std.core.String".to_string(), "trim".to_string()))
    );
    assert_eq!(qualified_import_member("trim(value)", 1), None);
}

#[test]
fn html_start_tag_at_skips_quoted_and_braced_greater_than_tokens() {
    let source = r#"<UserCard title="a > b" count={total > 1} disabled>"#;
    let offset = source.find("count").expect("count offset");

    let (tag, attrs) = html_start_tag_at(source, offset).expect("start tag");
    assert_eq!(tag, "UserCard");
    assert_eq!(attrs, vec!["title", "count", "disabled"]);
}

#[test]
fn hover_record_field_type_resolves_struct_field_annotation() {
    let module = parse(
        r#"module app.Main.

pub struct User {
    display_name: String
}.
"#,
    );
    let source = "#User.display_name";
    let offset = source.find("display_name").expect("field offset");

    assert_eq!(
        hover_record_field_type(&module, source, offset),
        Some("String".to_string())
    );
}

#[test]
fn hover_component_prop_type_maps_uppercase_tag_to_snake_function() {
    let module = parse(
        r#"module app.Main.

user_card(title: String, count: Int): String ->
    title.
"#,
    );
    let source = r#"<UserCard title={name} count={total}>"#;
    let offset = source.find("count").expect("prop offset");

    assert_eq!(
        hover_component_prop_type(&module, source, offset),
        Some("count: Int".to_string())
    );
}

#[test]
fn hover_local_docs_resolves_type_function_and_field_docs() {
    let module = parse(
        r#"module app.Main.

/**
 * User identifier.
 */
pub type UserId = Int.

pub struct User {
    /**
     * User display name.
     */
    display_name: String
}.

/**
 * Formats a user.
 */
format_user(user: User): String ->
    user.display_name.
"#,
    );

    assert_eq!(
        hover_local_docs(&module, "UserId", 1),
        Some("User identifier.".to_string())
    );
    assert_eq!(
        hover_local_docs(&module, "format_user(value)", 2),
        Some("Formats a user.".to_string())
    );
    assert_eq!(
        hover_local_docs(&module, "#User.display_name", 8),
        Some("User display name.".to_string())
    );
}

#[test]
fn interface_item_docs_prefers_type_docs_then_public_function_docs() {
    let mut interface = empty_interface("std.core.Sample");
    interface
        .type_docs
        .insert("UserId".to_string(), vec!["Imported user id.".to_string()]);
    interface.functions.insert(
        ("render".to_string(), 1),
        FunctionSignature {
            name: "render".to_string(),
            generic_params: Vec::new(),
            params: vec![ParamSignature {
                name: "value".to_string(),
                annotation: "String".to_string(),
                is_mutable: false,
                default_text: None,
            }],
            return_type: "String".to_string(),
            generic_bounds: Vec::new(),
            receiver_method: false,
            receiver_mutable: false,
            public: true,
            docs: vec!["Renders a value.".to_string()],
        },
    );
    interface.traits.insert(
        "Show".to_string(),
        TraitSignature {
            name: "Show".to_string(),
            type_params: vec!["T".to_string()],
            super_traits: Vec::new(),
            methods: HashMap::new(),
            docs: vec!["Imported show trait.".to_string()],
        },
    );

    assert_eq!(
        interface_item_docs(&interface, "UserId"),
        Some("Imported user id.".to_string())
    );
    assert_eq!(
        interface_item_docs(&interface, "render"),
        Some("Renders a value.".to_string())
    );
    assert_eq!(
        interface_item_docs(&interface, "Show"),
        Some("Imported show trait.".to_string())
    );
}

#[test]
fn hover_imported_docs_resolves_aliased_and_qualified_members() {
    let module = parse(
        r#"module app.Main.

import std.core.Sample.{UserId as Id, render}.
"#,
    );
    let mut interface = empty_interface("std.core.Sample");
    interface
        .type_docs
        .insert("UserId".to_string(), vec!["Imported user id.".to_string()]);
    interface.functions.insert(
        ("render".to_string(), 1),
        FunctionSignature {
            name: "render".to_string(),
            generic_params: Vec::new(),
            params: Vec::new(),
            return_type: "String".to_string(),
            generic_bounds: Vec::new(),
            receiver_method: false,
            receiver_mutable: false,
            public: true,
            docs: vec!["Imported renderer.".to_string()],
        },
    );
    let interfaces = HashMap::from([("std.core.Sample".to_string(), interface)]);

    assert_eq!(
        hover_imported_docs(&module, &interfaces, "Id", 1),
        Some("Imported user id.".to_string())
    );
    let qualified = "std.core.Sample.render";
    let offset = qualified.find("render").expect("member offset");
    assert_eq!(
        hover_imported_docs(&module, &interfaces, qualified, offset),
        Some("Imported renderer.".to_string())
    );
}
