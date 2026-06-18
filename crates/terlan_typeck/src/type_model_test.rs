use std::collections::{HashMap, HashSet};

use super::*;
use terlan_hir::resolve_syntax_module_output;
use terlan_syntax::parse_module_as_syntax_output;

/// Verifies multi-segment module type references keep their full module path.
///
/// Inputs:
/// - A type expression with a lowercase package segment, uppercase module
///   segment, and uppercase type name.
///
/// Output:
/// - Parsed and rendered type text preserving `people.Provider` as the
///   module path and `ExternalUser` as the type name.
///
/// Transformation:
/// - Exercises qualified type-name splitting so imported interface
///   conformance metadata can match consumer-side imported value types.
#[test]
fn type_parser_preserves_multi_segment_module_type_references() {
    let mut vars = HashMap::new();
    let mut next_var = 0usize;
    let ty = parse_type_expr(
        "people.Provider.ExternalUser",
        &HashSet::new(),
        &mut vars,
        &mut next_var,
    )
    .expect("parse qualified type");

    assert_eq!(pretty_type(&ty), "people.Provider.ExternalUser");
}

#[test]
fn syntax_output_collects_type_aliases_on_formal_path() {
    let module = parse_module_as_syntax_output(
        r#"
module aliases.

pub type Status = :active | :disabled.
pub type Boxed[T] = List[T].

pub struct User {
    id: Int,
    tags: Boxed[Binary]
}.

pub trait Named[T] {
    name(value: T): Binary.
}.

pub trait Show[T] extends Named[T] {
    show(value: T): Binary.
}.

template Profile from "./profile.terl.html" {
    title: Binary,
    user: User
}.

pub constructor Boxed[T] {
    (items: List[T]): Boxed[T] ->
        items;
    (...items: T): Boxed[T] ->
        items
}.

pub ok(): Status ->
    :active.

pub tag_count(tags: Boxed[Binary]): Int ->
    0.
"#,
    )
    .expect("parse syntax output type alias fixture");

    let aliases = collect_syntax_type_aliases(&module);
    let imported_aliases = HashMap::new();
    let imported_names = HashMap::new();
    let extra_names = collect_syntax_alias_extra_names(&module);
    let alias_names = collect_syntax_type_names(&module);
    let function_signatures = collect_syntax_function_signatures(
        &module,
        &alias_names,
        &imported_names,
        &imported_aliases,
        &aliases,
    );
    let constructor_signatures = collect_syntax_constructor_signatures(
        &module,
        &alias_names,
        &imported_names,
        &imported_aliases,
        &aliases,
    );
    let struct_fields = collect_syntax_struct_fields(&module, &alias_names);
    let template_schemes = collect_syntax_template_schemes(&module, &alias_names);
    let resolved = resolve_syntax_module_output(&module).module;
    let trait_signatures = collect_syntax_trait_signatures(&module, &resolved);

    let status = aliases.get("Status").expect("Status alias");
    assert!(matches!(
        &status.body,
        Type::Union(types)
            if types.contains(&Type::LiteralAtom("active".to_string()))
                && types.contains(&Type::LiteralAtom("disabled".to_string()))
    ));
    assert_eq!(aliases.get("Boxed").expect("Boxed alias").params.len(), 1);
    assert!(extra_names.contains("User"));
    let ok_signature = function_signatures
        .get(&("ok".to_string(), 0))
        .and_then(|signatures| signatures.first())
        .expect("ok function signature");
    assert_eq!(
        ok_signature.ret,
        Type::Named {
            module: None,
            name: "Status".to_string(),
            args: Vec::new(),
        }
    );
    let tag_count_signature = function_signatures
        .get(&("tag_count".to_string(), 1))
        .and_then(|signatures| signatures.first())
        .expect("tag_count function signature");
    assert_eq!(
        tag_count_signature.params,
        vec![Type::Named {
            module: None,
            name: "Boxed".to_string(),
            args: vec![Type::Binary],
        }]
    );
    assert_eq!(tag_count_signature.ret, Type::Int);
    let boxed_constructors = constructor_signatures
        .get("Boxed")
        .expect("Boxed constructor signatures");
    assert_eq!(boxed_constructors.len(), 2);
    assert_eq!(
        boxed_constructors[0].fixed_params,
        vec![Type::List(Box::new(Type::Var(0)))]
    );
    assert_eq!(boxed_constructors[0].min_arity, 1);
    assert_eq!(boxed_constructors[0].vararg, None);
    assert_eq!(
        boxed_constructors[0].ret,
        Type::List(Box::new(Type::Var(0)))
    );
    assert_eq!(boxed_constructors[1].fixed_params, Vec::<Type>::new());
    assert_eq!(boxed_constructors[1].min_arity, 0);
    assert_eq!(boxed_constructors[1].vararg, Some(Type::Var(0)));
    assert_eq!(
        boxed_constructors[1].ret,
        Type::List(Box::new(Type::Var(0)))
    );
    assert_eq!(
        struct_fields
            .get("User")
            .and_then(|fields| fields.get("id")),
        Some(&Type::Int)
    );
    assert_eq!(
        struct_fields
            .get("User")
            .and_then(|fields| fields.get("tags")),
        Some(&Type::Named {
            module: None,
            name: "Boxed".to_string(),
            args: vec![Type::Binary],
        })
    );
    assert_eq!(
        template_schemes
            .get("Profile")
            .and_then(|scheme| scheme.props.get("title")),
        Some(&Type::Binary)
    );
    assert_eq!(
        template_schemes
            .get("Profile")
            .and_then(|scheme| scheme.props.get("user")),
        Some(&Type::Named {
            module: None,
            name: "User".to_string(),
            args: Vec::new(),
        })
    );
    let show_trait = trait_signatures.get("Show").expect("Show trait signature");
    assert_eq!(show_trait.type_params, vec!["T".to_string()]);
    assert_eq!(show_trait.super_traits, vec!["Named[T]".to_string()]);
    let show_method = show_trait.methods.get("show").expect("show method");
    assert_eq!(
        show_method
            .params
            .iter()
            .map(|param| param.ty.as_str())
            .collect::<Vec<_>>(),
        vec!["T"]
    );
    assert_eq!(show_method.return_type, "Binary");
}
