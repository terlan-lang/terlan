//! Focused checks for representation-safe checked-cast lowering.

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{lower_syntax_module_output_to_core, CoreExpr, CoreModule, CoreType};

use super::native_object_test_support::{
    assert_managed_native_object_invocations, NativeObjectInvocation,
};
use super::{emit_native_application_object, status};
use super::{NativeExpr, NativeModule};

fn cast_module(target_type: CoreType) -> CoreModule {
    let syntax = parse_module_as_syntax_output(
        "module checked_cast.\n\npub cast(value: Int): Int -> value.\n",
    )
    .expect("parse checked-cast source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
    *core.functions[0].clauses[0]
        .body
        .core_expr
        .as_mut()
        .expect("checked body") = CoreExpr::Cast {
        expr: Box::new(CoreExpr::Var("value".to_string())),
        target_type,
    };
    core
}

#[test]
fn representation_preserving_checked_cast_is_erased_in_native_ir() {
    let core = cast_module(CoreType::Int);
    let modules = NativeModule::lower_application(&[&core]).expect("checked cast NativeIR");

    assert_eq!(modules[0].functions[0].body, NativeExpr::Param(0));
}

#[test]
fn representation_changing_checked_cast_fails_before_linking() {
    let core = cast_module(CoreType::Bool);
    let error = NativeModule::lower_application(&[&core]).expect_err("reject incompatible cast");

    assert!(error.starts_with("error[native_ir.cast_check]"), "{error}");
}

#[test]
fn nullary_union_cast_inference_requires_one_matching_empty_variant() {
    use super::constructors::{install_structural_type_layouts, zero_field_managed_variant_layout};
    use super::expression::infer_native_type_with_constructors;
    use crate::terlan_typeck::core_type_from_text;
    use std::collections::HashMap;

    let ty = core_type_from_text("Atom[\"done\"] | {Atom[\"value\"], Int}").unwrap();
    let target = super::native_type(Some(&ty), "").unwrap();
    let mut layouts = HashMap::new();
    install_structural_type_layouts([&ty], &mut layouts).unwrap();
    assert_eq!(
        infer_native_type_with_constructors(
            &CoreExpr::Cast {
                expr: Box::new(CoreExpr::Var("done".into())),
                target_type: ty.clone()
            },
            &HashMap::from([("done".into(), super::NativeType::Int)]),
            &HashMap::new(),
            &layouts,
        ),
        None,
        "a bound value must not turn into a similarly named variant"
    );
    for (name, expected) in [("done", Some(target)), ("value", None), ("missing", None)] {
        let expr = CoreExpr::Cast {
            expr: Box::new(CoreExpr::Atom(name.into())),
            target_type: ty.clone(),
        };
        assert_eq!(
            infer_native_type_with_constructors(&expr, &HashMap::new(), &HashMap::new(), &layouts,),
            expected,
            "{name}"
        );
    }
    let atom = CoreExpr::Atom("done".into());
    let mut conflicting = zero_field_managed_variant_layout(&atom, target, &layouts)
        .unwrap()
        .unwrap()
        .clone();
    conflicting.encoded_layout = std::sync::Arc::from(b"conflicting layout".as_slice());
    layouts.insert(("conflict".into(), 0), conflicting);
    assert!(zero_field_managed_variant_layout(&atom, target, &layouts)
        .unwrap_err()
        .to_string()
        .contains("native_ir.constructor_variant"));
    assert_eq!(
        infer_native_type_with_constructors(
            &CoreExpr::Cast {
                expr: Box::new(atom),
                target_type: ty
            },
            &HashMap::new(),
            &HashMap::new(),
            &layouts,
        ),
        None
    );
}

#[test]
fn checked_conditional_cast_applies_the_result_type_to_every_branch() {
    use crate::terlan_typeck::CoreIfClause;
    for incompatible in [false, true] {
        let mut core = cast_module(CoreType::Int);
        let CoreExpr::Cast { expr, .. } = core.functions[0].clauses[0]
            .body
            .core_expr
            .as_mut()
            .unwrap()
        else {
            panic!("cast fixture");
        };
        **expr = CoreExpr::If {
            clauses: vec![
                CoreIfClause {
                    condition: CoreExpr::Var("true".into()),
                    body: CoreExpr::Var("value".into()),
                },
                CoreIfClause {
                    condition: CoreExpr::Var("true".into()),
                    body: if incompatible {
                        CoreExpr::Var("false".into())
                    } else {
                        CoreExpr::Int(7)
                    },
                },
            ],
        };
        let result = NativeModule::lower_application(&[&core]);
        if incompatible {
            assert!(result.unwrap_err().contains("native_ir.cast_check"));
        } else {
            let modules = result.unwrap();
            assert!(
                matches!(&modules[0].functions[0].body, NativeExpr::If { clauses } if clauses.len() == 2)
            );
        }
    }
}

#[test]
fn conditional_option_results_keep_context_after_source_helper_inlining() {
    super::source_constructor_test::check_sources(&[
        r#"
module conditional_option_context.
import std.core.Option.{Some, None, with_default}.
import type std.core.Option.
pub struct Value { #text: String, #present: Bool }.
optional(value: Value): Option[String] ->
    if { value.#present -> Some(value.#text); true -> None }.
selected(value: Value): String ->
    case optional(value) { Some(text) -> text; None -> "missing" }.
pub check(): Bool ->
    selected(Value { #text: "kept", #present: true }) == "kept"
        and selected(Value { #text: "", #present: true }) == ""
        and selected(Value { #text: "ignored", #present: false }) == "missing"
        and with_default(optional(Value { #text: "retained", #present: true }), "fallback") == "retained".
"#,
        include_str!("../../../../../std/core/Option.terl"),
    ]);
}

fn boxed(value: CoreExpr) -> CoreExpr {
    CoreExpr::Cast {
        expr: Box::new(value),
        target_type: CoreType::Named("$aot.erased_value".to_string()),
    }
}

fn unboxed(value: CoreExpr, target_type: CoreType) -> CoreExpr {
    CoreExpr::Cast {
        expr: Box::new(value),
        target_type,
    }
}

fn run_erased_cast(
    source: &str,
    body: CoreExpr,
    arguments: Vec<i64>,
    expected_status: i32,
    expected: Option<i64>,
) -> usize {
    let syntax = parse_module_as_syntax_output(source).expect("parse typed erasure fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let mut core = lower_syntax_module_output_to_core(&syntax, &resolved);
    *core.functions[0].clauses[0]
        .body
        .core_expr
        .as_mut()
        .expect("checked body") = body;
    let modules =
        NativeModule::lower_application(&[&core]).expect("lower checked existential casts");
    let object = emit_native_application_object(&core.module, &modules).expect("emit boxed casts");
    assert_managed_native_object_invocations(
        &core.module,
        &modules,
        &object,
        &[NativeObjectInvocation {
            export_id: modules[0].functions[0].export_id,
            arguments,
            expected_status,
            expected_result: expected,
        }],
    );
    modules
        .iter()
        .map(|module| module.continuations.len())
        .sum()
}

#[test]
fn erased_casts_preserve_scalar_words_in_linked_aot_code() {
    for (ty, word) in [
        (CoreType::Int, i64::MIN),
        (
            CoreType::Float,
            i64::from_ne_bytes(0x7ff8_0000_0000_0042_u64.to_ne_bytes()),
        ),
        (CoreType::Bool, 1),
    ] {
        let source = format!(
            "module erased_cast.\npub cast(value: {0}): {0} -> value.\n",
            ty.contract_text()
        );
        run_erased_cast(
            &source,
            unboxed(boxed(CoreExpr::Var("value".to_string())), ty),
            vec![word],
            status::OK,
            Some(word),
        );
    }
}

#[test]
fn erased_casts_reject_wrong_runtime_type_even_when_word_is_valid() {
    run_erased_cast(
        "module erased_wrong_type.\npub cast(value: Int): Bool -> true.\n",
        unboxed(boxed(CoreExpr::Var("value".to_string())), CoreType::Bool),
        vec![1],
        crate::runtime::native_image::managed::MANAGED_ALLOCATION_FAILED_STATUS,
        None,
    );
}

#[test]
fn erased_casts_round_trip_managed_values_without_retyping_them() {
    let syntax = parse_module_as_syntax_output(
        "module erased_string.\npub cast(): Bool -> \"retained\" == \"retained\".\n",
    )
    .expect("parse managed-value equality");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let mut body = core.functions[0].clauses[0]
        .body
        .core_expr
        .clone()
        .expect("equality body");
    let CoreExpr::BinaryOp { left, .. } = &mut body else {
        panic!("expected binary equality");
    };
    **left = unboxed(boxed((**left).clone()), CoreType::String);
    run_erased_cast(
        "module erased_string.\npub cast(): Bool -> true.\n",
        body,
        vec![],
        status::OK,
        Some(1),
    );
}

#[test]
fn ordinary_dynamic_cast_is_not_an_existential_escape_hatch() {
    let core = cast_module(CoreType::Dynamic);
    let error = NativeModule::lower_application(&[&core]).expect_err("reject untyped Dynamic cast");
    assert!(error.contains("error[native_ir."), "{error}");
}

#[test]
fn erased_managed_payload_survives_a_scheduler_continuation() {
    let source = "module erased_suspended.\nimport std.vm.Process.\n\
        pub cast(): Bool -> let saved = \"retained\";\
        let _pause = Process.yield_now(); saved == \"retained\".\n";
    let syntax = parse_module_as_syntax_output(source).expect("parse suspending erasure fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let mut body = core.functions[0].clauses[0]
        .body
        .core_expr
        .clone()
        .expect("let body");
    let CoreExpr::Let {
        bindings,
        body: result,
    } = &mut body
    else {
        panic!("expected let");
    };
    bindings[0].value = boxed(bindings[0].value.clone());
    let CoreExpr::BinaryOp { left, .. } = result.as_mut() else {
        panic!("expected equality");
    };
    **left = unboxed((**left).clone(), CoreType::String);
    let continuations = run_erased_cast(source, body, vec![], status::OK, Some(1));
    assert!(
        continuations > 0,
        "the fixture must cross a scheduler continuation"
    );
}

#[test]
fn erased_nullary_union_survives_a_native_call_and_scheduler_continuation() {
    let source = "module erased_nullary.\n\
        import std.core.Option.{Option, None, Some}.\n\
        import std.vm.Process.\n\
        pub cast(): Bool -> let saved = missing();\
        let _pause = Process.yield_now();\
        case saved { None -> true; Some(_) -> false }.\n\
        missing(): Option[Int] -> None.\n";
    let syntax = parse_module_as_syntax_output(source).expect("parse nullary erasure fixture");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let mut body = core.functions[0].clauses[0]
        .body
        .core_expr
        .clone()
        .expect("let body");
    let CoreExpr::Let {
        bindings,
        body: result,
    } = &mut body
    else {
        panic!("expected let");
    };
    bindings[0].value = boxed(bindings[0].value.clone());
    let CoreExpr::Case { scrutinee, .. } = result.as_mut() else {
        panic!("expected case");
    };
    **scrutinee = unboxed(
        (**scrutinee).clone(),
        CoreType::Apply {
            constructor: "Option".to_string(),
            args: vec![CoreType::Int],
        },
    );
    assert!(run_erased_cast(source, body, vec![], status::OK, Some(1)) > 0);
}
