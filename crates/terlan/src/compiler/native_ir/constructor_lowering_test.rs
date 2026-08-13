use std::collections::HashMap;

use crate::runtime::native_image::managed::ManagedFieldType;
use crate::terlan_hir::{
    checked_in_std_interfaces_for_module, resolve_syntax_module_output,
    resolve_syntax_module_output_with_interfaces,
};
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{
    lower_syntax_module_output_to_core, type_check_syntax_module_output, CoreConstructorDecl,
    CoreExpr, CoreParam, CoreType,
};

use super::constructor_chain::lower_constructor_chains;
use super::constructors::native_constructor_layouts;
use super::expression::lower_expr_with_constructors;
use super::native_object_test_support::{
    assert_managed_native_object_invocations, NativeObjectInvocation,
};
use super::{emit_native_application_object, status, NativeExpr, NativeModule, NativeType};

#[test]
fn transparent_generic_variant_return_keeps_the_declared_union_layout() {
    let syntax = parse_module_as_syntax_output(
        r#"
module native_constructor_return.

import std.core.Option.{None, Some}.
import type std.core.Option.Option.

pub classify(value: String): Option[String] ->
    if {
        value == "present" -> Some("selected");
        true -> Some("fallback")
    }.

pub selected(): Bool ->
    case classify("present") {
        Some(value) -> value == "selected";
        None -> false
    }.
"#,
    )
    .expect("parse transparent constructor return fixture");
    let interfaces = checked_in_std_interfaces_for_module(&syntax);
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("lower constructor return");
    let export_id = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "selected")
        .expect("selected export")
        .export_id;
    let object = emit_native_application_object("native_constructor_return", &modules)
        .expect("emit constructor return object");

    assert_managed_native_object_invocations(
        "native-constructor-return",
        &modules,
        &object,
        &[NativeObjectInvocation {
            export_id,
            arguments: Vec::new(),
            expected_status: status::OK,
            expected_result: Some(1),
        }],
    );
}

/// Builds one fixed constructor declaration for a shared `Result[Int, Int]` union.
fn declaration(name: &str, parameter: &str) -> CoreConstructorDecl {
    CoreConstructorDecl {
        name: name.to_owned(),
        public: true,
        min_arity: 1,
        params: vec![CoreParam {
            name: parameter.to_owned(),
            ty: "Int".to_owned(),
            core_ty: Some(CoreType::Int),
        }],
        vararg: None,
        return_type: "Result[Int, Int]".to_owned(),
        core_return_type: Some(CoreType::Apply {
            constructor: "Result".to_owned(),
            args: vec![CoreType::Int, CoreType::Int],
        }),
    }
}

#[test]
fn fixed_constructor_calls_lower_to_canonical_managed_native_ir() {
    let declarations = vec![declaration("Ok", "value"), declaration("Error", "reason")];
    let modules = [("result", declarations.as_slice())];
    let layouts = native_constructor_layouts(&modules, "result").expect("constructor layouts");
    let call = CoreExpr::ConstructorCall {
        constructor: "Ok".to_owned(),
        constructor_identity: Some("result.Ok".to_owned()),
        args: vec![CoreExpr::Int(42)],
    };
    let lowered = lower_expr_with_constructors(
        &call,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &layouts,
    )
    .expect("constructor lowering");

    let NativeExpr::Construct {
        descriptor,
        encoded_layout,
        fields,
    } = lowered
    else {
        panic!("expected managed constructor NativeIR");
    };
    assert_eq!(descriptor.discriminant(), Some(1));
    assert_eq!(descriptor.variant_count(), Some(2));
    assert_eq!(descriptor.fields()[0].name(), Some("value"));
    assert_eq!(descriptor.fields()[0].field_type(), ManagedFieldType::Int);
    assert_eq!(fields, vec![NativeExpr::Int(42)]);
    assert_eq!(
        crate::runtime::native_image::managed::decode_aggregate_layout(&encoded_layout)
            .expect("encoded NativeIR layout"),
        *descriptor
    );
    assert!(matches!(
        layouts
            .get(&("Ok".to_owned(), 1))
            .map(|layout| layout.result),
        Some(NativeType::ManagedRef(_))
    ));
}

#[test]
fn constructor_layouts_are_stable_across_declaration_order() {
    let forward = vec![declaration("Ok", "value"), declaration("Error", "reason")];
    let reverse = vec![declaration("Error", "reason"), declaration("Ok", "value")];
    let first = native_constructor_layouts(&[("result", forward.as_slice())], "consumer")
        .expect("forward layouts");
    let second = native_constructor_layouts(&[("result", reverse.as_slice())], "consumer")
        .expect("reverse layouts");

    assert_eq!(first, second);
    assert!(!first.contains_key(&("Ok".to_owned(), 1)));
    assert!(first.contains_key(&("result.Ok".to_owned(), 1)));
}

/// Verifies explicit transparent-record constructors retain nominal identity.
///
/// Imported signatures carry a record's module-qualified name while its
/// visible body carries the full structural shape. Both forms must lower to
/// one semantic ID so record values survive native suspension boundaries.
#[test]
fn transparent_record_constructor_uses_qualified_nominal_semantic_identity() {
    let canonical = "std.range.Range.Range";
    let declaration = CoreConstructorDecl {
        name: "Range".to_owned(),
        public: true,
        min_arity: 4,
        params: vec![
            CoreParam {
                name: "start".to_owned(),
                ty: "Int".to_owned(),
                core_ty: Some(CoreType::Int),
            },
            CoreParam {
                name: "stop".to_owned(),
                ty: "Int".to_owned(),
                core_ty: Some(CoreType::Int),
            },
            CoreParam {
                name: "step".to_owned(),
                ty: "Int".to_owned(),
                core_ty: Some(CoreType::Int),
            },
            CoreParam {
                name: "inclusive".to_owned(),
                ty: "Bool".to_owned(),
                core_ty: Some(CoreType::Bool),
            },
        ],
        vararg: None,
        return_type: "Range".to_owned(),
        core_return_type: Some(CoreType::Struct {
            name: canonical.to_owned(),
            fields: vec![
                crate::terlan_typeck::CoreStructTypeField {
                    name: "start".to_owned(),
                    ty: CoreType::Int,
                    is_private: false,
                },
                crate::terlan_typeck::CoreStructTypeField {
                    name: "stop".to_owned(),
                    ty: CoreType::Int,
                    is_private: false,
                },
                crate::terlan_typeck::CoreStructTypeField {
                    name: "step".to_owned(),
                    ty: CoreType::Int,
                    is_private: false,
                },
                crate::terlan_typeck::CoreStructTypeField {
                    name: "inclusive".to_owned(),
                    ty: CoreType::Bool,
                    is_private: false,
                },
            ],
        }),
    };

    let layouts = native_constructor_layouts(
        &[("std.range.Range", std::slice::from_ref(&declaration))],
        "consumer",
    )
    .expect("transparent record constructor layout");
    let layout = &layouts[&("std.range.Range.Range".to_owned(), 4)];
    let expected = crate::runtime::native_image::managed::SemanticTypeId::from_canonical(canonical)
        .expect("nominal semantic");

    assert_eq!(layout.descriptor.canonical_type(), canonical);
    assert_eq!(layout.descriptor.discriminant(), None);
    assert_eq!(layout.result, NativeType::ManagedRef(expected));
    assert_eq!(layout.descriptor.managed().semantic_id(), expected);
}

#[test]
fn unresolved_and_vararg_constructors_are_rejected_without_partial_lowering() {
    let mut vararg = declaration("Many", "first");
    vararg.vararg = Some(CoreParam {
        name: "rest".to_owned(),
        ty: "Int".to_owned(),
        core_ty: Some(CoreType::Int),
    });
    let layouts = native_constructor_layouts(
        &[("result", &[declaration("Ok", "value"), vararg])],
        "result",
    )
    .expect("vararg declaration inventory");
    assert!(layouts.is_empty());

    let call = CoreExpr::ConstructorCall {
        constructor: "Missing".to_owned(),
        constructor_identity: None,
        args: vec![CoreExpr::Int(1)],
    };
    let error = lower_expr_with_constructors(
        &call,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &layouts,
    )
    .expect_err("unknown constructor must fail");
    assert!(error.contains("fixed constructor `Missing/1` has no native layout"));
}

#[test]
fn constructor_lowering_rejects_a_field_that_disagrees_with_checked_layout() {
    let declarations = vec![declaration("Ok", "value"), declaration("Error", "reason")];
    let layouts = native_constructor_layouts(&[("result", declarations.as_slice())], "result")
        .expect("constructor layouts");
    let call = CoreExpr::ConstructorCall {
        constructor: "Ok".to_owned(),
        constructor_identity: Some("result.Ok".to_owned()),
        args: vec![CoreExpr::Atom("true".to_owned())],
    };
    let error = lower_expr_with_constructors(
        &call,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &layouts,
    )
    .expect_err("field type mismatch must fail");

    assert!(
        error.starts_with("error[native_ir.collection_value]"),
        "{error}"
    );
    assert!(error.contains("expected Int"), "{error}");
    assert!(error.contains("found Bool"), "{error}");
}

#[test]
fn constructor_chain_is_eliminated_before_native_admission() {
    let syntax = parse_module_as_syntax_output(
        "module constructor_chain_native.\n\n\
         pub constructor User {\n\
             (id: Int): Dynamic -> id\n\
         }.\n\n\
         pub make(id: Int): Dynamic ->\n\
             User(id) with Admin { id: id }.\n",
    )
    .expect("parse constructor-chain source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let mut core = crate::terlan_typeck::lower_syntax_module_output_to_core(&syntax, &resolved);
    let original = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .and_then(|function| function.clauses[0].body.core_expr.as_ref())
        .expect("constructor-chain body");
    assert!(matches!(original, CoreExpr::ConstructorChain { .. }));

    lower_constructor_chains(&mut core);

    let rewritten = core
        .functions
        .iter()
        .find(|function| function.name == "make")
        .and_then(|function| function.clauses[0].body.core_expr.as_ref())
        .expect("rewritten constructor-chain body");
    let CoreExpr::Let { bindings, body } = rewritten else {
        panic!("constructor chain survived mandatory rewrite")
    };
    assert!(matches!(
        bindings.as_slice(),
        [crate::terlan_typeck::CoreLetBinding {
            pattern: crate::terlan_typeck::CorePattern::Var(name),
            value: CoreExpr::ConstructorCall {
                constructor,
                constructor_identity: Some(identity),
                ..
            },
        }] if name == "$native_constructor_chain_0"
            && constructor == "User"
            && identity == "User"
    ));
    assert!(matches!(body.as_ref(), CoreExpr::RecordConstruct { name, .. } if name == "Admin"));
}
