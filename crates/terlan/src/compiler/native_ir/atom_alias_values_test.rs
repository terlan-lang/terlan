use std::collections::HashMap;

use crate::terlan_typeck::CorePattern;

use super::{rewrite, rewrite_pattern, AliasValue};

#[test]
fn bare_type_constructor_pattern_becomes_its_canonical_atom() {
    let mut pattern = CorePattern::Constructor {
        name: "std.binary.Binary.TruncatedPayload".to_string(),
        constructor_identity: Some("std.binary.Binary.TruncatedPayload".to_string()),
        args: Vec::new(),
    };

    rewrite_pattern(
        &mut pattern,
        &HashMap::from([(
            "TruncatedPayload".to_string(),
            AliasValue {
                atom: "truncatedpayload".to_string(),
            },
        )]),
    );

    assert_eq!(pattern, CorePattern::Atom("truncatedpayload".to_string()));
}

#[test]
fn atom_alias_value_and_pattern_keep_the_same_semantic_identity() {
    let aliases = HashMap::from([(
        "None".to_string(),
        AliasValue {
            atom: "none".to_string(),
        },
    )]);
    let mut expression = crate::terlan_typeck::CoreExpr::Var("None".to_string());
    let mut pattern = CorePattern::Constructor {
        name: "None".to_string(),
        constructor_identity: Some("std.core.Option.None".to_string()),
        args: Vec::new(),
    };

    rewrite(&mut expression, &aliases);
    rewrite_pattern(&mut pattern, &aliases);

    assert_eq!(
        expression,
        crate::terlan_typeck::CoreExpr::Atom("none".to_string())
    );
    assert_eq!(pattern, CorePattern::Atom("none".to_string()));
}

#[test]
fn unit_alias_keeps_the_native_unit_sentinel() {
    let aliases = HashMap::from([(
        "Unit".to_string(),
        AliasValue {
            atom: "unit".to_string(),
        },
    )]);
    let mut expression = crate::terlan_typeck::CoreExpr::Var("Unit".to_string());

    rewrite(&mut expression, &aliases);

    assert_eq!(
        expression,
        crate::terlan_typeck::CoreExpr::Atom("Unit".to_string())
    );
}

#[test]
fn atom_alias_patterns_execute_standalone_and_in_multiple_managed_unions() {
    use crate::compiler::native_ir::{
        emit_native_application_object,
        native_object_test_support::{
            assert_managed_native_object_invocations, NativeObjectInvocation,
        },
        status, NativeModule,
    };
    use crate::{
        terlan_hir::resolve_syntax_module_output,
        terlan_syntax::parse_module_as_syntax_output,
        terlan_typeck::{lower_syntax_module_output_to_core, type_check_syntax_module_output},
    };

    let source = r#"
module atom_union_patterns.
pub type None.
pub type Some[T] = {Atom["some"], value: T}.
pub type Option[T] = None | Some[T].
pub type Missing = Atom["absent"].
pub type Payload = {Atom["payload"], Int}.
pub type Answer = Payload | Missing.
int_option(present: Bool): Option[Int] ->
    if { present -> Some(41); true -> None }.
bool_option(present: Bool): Option[Bool] ->
    if { present -> Some(true); true -> None }.
answer(present: Bool): Answer ->
    if { present -> Payload(7); true -> Missing }.
pub standalone(): Int ->
    let present = case Some(41) { Some(value) -> value + 1; None -> 0 };
    let missing = case None { Some(_value) -> 0; None -> 1 };
    present + missing.
pub managed_int(present: Bool): Int ->
    case int_option(present) { Some(value) -> value; None -> 0 }.
pub managed_bool(present: Bool): Bool ->
    case bool_option(present) { Some(value) -> value; None -> false }.
pub other_union(present: Bool): Int ->
    case answer(present) { Missing -> 0; Payload(value) -> value }.
"#;
    let syntax = parse_module_as_syntax_output(source).expect("parse atom union patterns");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower atom union patterns");
    let object = emit_native_application_object("atom-union-patterns", &modules)
        .expect("emit atom union patterns");
    let invocations = [
        ("standalone", vec![], 43),
        ("managed_int", vec![1], 41),
        ("managed_int", vec![0], 0),
        ("managed_bool", vec![1], 1),
        ("managed_bool", vec![0], 0),
        ("other_union", vec![1], 7),
        ("other_union", vec![0], 0),
    ]
    .into_iter()
    .map(|(name, arguments, expected)| NativeObjectInvocation {
        export_id: modules
            .iter()
            .flat_map(|module| &module.functions)
            .find(|function| function.name == name)
            .expect("pattern export")
            .export_id,
        arguments,
        expected_status: status::OK,
        expected_result: Some(expected),
    })
    .collect::<Vec<_>>();
    assert_managed_native_object_invocations(
        "atom-union-patterns",
        &modules,
        &object,
        &invocations,
    );
}
