//! Seeded optimized/unoptimized native executions with independent scalar oracles.

use super::*;
use crate::compiler::native_ir::native_object_test_support::{
    assert_managed_native_object_invocations, NativeObjectInvocation,
};
use crate::compiler::native_ir::{lower_expr_with_constructors, status, NativeType};
use std::collections::HashMap;

fn binary(operator: &str, left: CoreExpr, right: CoreExpr) -> CoreExpr {
    CoreExpr::BinaryOp {
        operator: operator.into(),
        left: Box::new(left),
        right: Box::new(right),
    }
}

/// Deterministic cases include failures in discarded fields and lexical shadowing.
fn generated_case(seed: u64) -> (CoreExpr, i32, Option<i64>) {
    let mixed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    let left = ((mixed >> 32) % 2001) as i64 - 1000;
    let right = ((mixed >> 16) % 2001) as i64 - 1000;
    let mut first = binary("+", CoreExpr::Int(left), CoreExpr::Int(3));
    let mut second = binary("*", CoreExpr::Int(right), CoreExpr::Int(2));
    let mut expected_status = status::OK;
    match seed % 8 {
        0 => {
            first = binary("div", CoreExpr::Int(1), CoreExpr::Int(0));
            expected_status = status::DIVISION_BY_ZERO;
        }
        1 => {
            first = binary("+", CoreExpr::Int(i64::MAX), CoreExpr::Int(1));
            expected_status = status::OVERFLOW;
        }
        2 => {
            second = binary("div", CoreExpr::Int(1), CoreExpr::Int(0));
            expected_status = status::DIVISION_BY_ZERO;
        }
        3 => {
            first = CoreExpr::If {
                clauses: vec![crate::terlan_typeck::CoreIfClause {
                    condition: CoreExpr::Atom("false".into()),
                    body: CoreExpr::Int(7),
                }],
            };
            expected_status = status::NO_MATCHING_BRANCH;
        }
        4 => {
            first = CoreExpr::UnaryOp {
                operator: "-".into(),
                operand: Box::new(CoreExpr::Int(i64::MIN)),
            };
            expected_status = status::OVERFLOW;
        }
        _ => {}
    }
    let (body, answer) = match (seed / 8) % 3 {
        0 => (
            binary("+", field("pair", "left"), field("pair", "right")),
            left + 3 + right * 2,
        ),
        1 => (field("pair", "left"), left + 3),
        _ => (
            CoreExpr::Let {
                bindings: vec![CoreLetBinding {
                    pattern: CorePattern::Var("pair".into()),
                    value: CoreExpr::Int(42),
                }],
                body: Box::new(CoreExpr::Var("pair".into())),
            },
            42,
        ),
    };
    (
        CoreExpr::Let {
            bindings: vec![CoreLetBinding {
                pattern: CorePattern::Var("pair".into()),
                value: pair(first, second),
            }],
            body: Box::new(body),
        },
        expected_status,
        (expected_status == status::OK).then_some(answer),
    )
}

/// Exercises generated native code, the real allocator/resolver, and exact failures.
#[test]
fn seeded_scalar_replacement_matches_unoptimized_execution() {
    let constructors = layouts();
    let mut module = NativeModule {
        name: "seeded_sroa".into(),
        functions: Vec::new(),
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    };
    let mut invocations = Vec::new();
    for seed in 0..96 {
        let (original, expected_status, expected_result) = generated_case(seed);
        let rewritten = scalar_replace_fixed_aggregates(&original, &constructors);
        for (optimized, core) in [(false, &original), (true, &rewritten)] {
            let body = lower_expr_with_constructors(
                core,
                &HashMap::new(),
                &HashMap::new(),
                &HashMap::new(),
                &HashMap::new(),
                &constructors,
            )
            .unwrap_or_else(|error| panic!("seed {seed}, optimized {optimized}: {error}"));
            let mut encodings = Vec::new();
            body.collect_managed_encodings(&mut encodings);
            let layouts = encodings
                .into_iter()
                .filter(|bytes| {
                    crate::runtime::native_image::managed::decode_aggregate_layout(bytes).is_ok()
                })
                .collect::<Vec<_>>();
            if optimized && (seed / 8) % 3 != 2 {
                assert!(
                    layouts.is_empty(),
                    "seed {seed}: optimization left an aggregate allocation: {body:?}"
                );
            }
            if !optimized && (seed / 8) % 3 != 2 {
                assert!(
                    !layouts.is_empty(),
                    "seed {seed}: baseline lost its live aggregate"
                );
            }
            module.managed_layouts.extend(layouts);
            let export_id = 1000 + seed * 2 + u64::from(optimized);
            let name = format!(
                "seed_{seed}_{}",
                if optimized { "optimized" } else { "baseline" }
            );
            module
                .functions
                .push(crate::compiler::native_ir::NativeFunction {
                    export_id,
                    name: name.clone(),
                    public: true,
                    arity: 0,
                    source_module: module.name.clone(),
                    source_function: name,
                    source_arity: 0,
                    callable_captures: Vec::new(),
                    params: Vec::new(),
                    return_type: NativeType::Int,
                    body,
                });
            invocations.push(NativeObjectInvocation {
                export_id,
                arguments: Vec::new(),
                expected_status,
                expected_result,
            });
        }
    }
    module.managed_layouts.sort();
    module.managed_layouts.dedup();
    let modules = [module];
    for policy in [
        crate::compiler::native_ir::NativeCodegenPolicy::Development,
        crate::compiler::native_ir::NativeCodegenPolicy::Serve,
        crate::compiler::native_ir::NativeCodegenPolicy::Release,
    ] {
        let object = crate::compiler::native_ir::emit_native_application_object_with_policy(
            "seeded_sroa",
            &modules,
            policy,
        )
        .expect("seeded native object");
        assert_managed_native_object_invocations(
            policy.cache_identity(),
            &modules,
            &object,
            &invocations,
        );
    }
}

/// Public String APIs exercise parsing, interfaces, checking, AOT linking and execution.
#[test]
fn compact_string_join_survives_the_full_native_compilation_cycle() {
    let fragments = (0..65).map(|index| format!("λ{index}")).collect::<Vec<_>>();
    let literals = fragments
        .iter()
        .map(|value| format!("\"{value}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let expected = fragments.concat();
    let source = format!(
        r#"
module compact_join.
import std.core.String.
pub check(): Bool ->
    let empty = [];
    String.concat([]) == "" and
    String.concat(empty) == "" and
    String.concat(["λ"]) == "λ" and
    String.concat([{literals}]) == "{expected}".
"#
    );
    crate::compiler::native_ir::source_constructor_test::check_sources(&[
        &source,
        include_str!("../../../../../std/core/String.terl"),
    ]);
}
