use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::terlan_hir::resolve_syntax_module_output;
use crate::terlan_syntax::parse_module_as_syntax_output;
use crate::terlan_typeck::{
    lower_syntax_module_output_to_core, type_check_syntax_module_output, CoreConstructorDecl,
    CoreExpr, CoreExprSummary, CoreFunction, CoreFunctionClause, CoreLetBinding, CoreParam,
    CorePattern, CoreProofCoverage, CoreType,
};

use super::constructors::{native_constructor_layouts, NativeConstructorLayouts};
use super::scalar_replacement::scalar_replace_fixed_aggregates;
use super::{
    emit_native_application_object, is_scalar_candidate, lower_native_function,
    NativeBinaryOperator, NativeExpr, NativeModule,
};

/// Builds one two-field fixed constructor layout for projection tests.
fn layouts() -> NativeConstructorLayouts {
    let result = CoreType::Apply {
        constructor: "Result".to_owned(),
        args: vec![CoreType::Int, CoreType::Int],
    };
    let declarations = vec![CoreConstructorDecl {
        name: "Pair".to_owned(),
        public: true,
        min_arity: 2,
        params: vec![
            CoreParam {
                name: "left".to_owned(),
                ty: "Int".to_owned(),
                core_ty: Some(CoreType::Int),
            },
            CoreParam {
                name: "right".to_owned(),
                ty: "Int".to_owned(),
                core_ty: Some(CoreType::Int),
            },
        ],
        vararg: None,
        return_type: "Result[Int, Int]".to_owned(),
        core_return_type: Some(result),
    }];
    native_constructor_layouts(&[("projection", declarations.as_slice())], "projection")
        .expect("projection layouts")
}

/// Creates one resolved two-field constructor call.
fn pair(left: CoreExpr, right: CoreExpr) -> CoreExpr {
    CoreExpr::ConstructorCall {
        constructor: "Pair".to_owned(),
        constructor_identity: Some("projection.Pair".to_owned()),
        args: vec![left, right],
    }
}

/// Creates one named field projection from a local aggregate.
fn field(local: &str, name: &str) -> CoreExpr {
    CoreExpr::FieldAccess {
        base: Box::new(CoreExpr::Var(local.to_owned())),
        field: name.to_owned(),
    }
}

/// Creates a zero-arity function around one test body.
fn function(body: CoreExpr) -> CoreFunction {
    CoreFunction {
        name: "projected".to_owned(),
        arity: 0,
        public: true,
        generic_params: Vec::new(),
        native_operation: None,
        params: Vec::new(),
        return_type: "Int".to_owned(),
        core_return_type: Some(CoreType::Int),
        clauses: vec![CoreFunctionClause {
            patterns: Vec::new(),
            core_patterns: Vec::new(),
            pattern_proof_coverage: Vec::new(),
            pattern_checked_preservation_evidence: Vec::new(),
            guard: None,
            body: CoreExprSummary {
                kind: "scalar-replacement-test".to_owned(),
                core_expr: Some(body),
                checked_preservation_evidence: None,
                proof_coverage: CoreProofCoverage::LeanCovered,
                text: None,
                remote: None,
                operator: None,
                arity: 0,
                children: Vec::new(),
            },
        }],
    }
}

/// Builds the canonical projected sum used by production regressions.
fn projected_sum() -> CoreExpr {
    CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("pair".to_owned()),
            value: pair(CoreExpr::Int(20), CoreExpr::Int(22)),
        }],
        body: Box::new(CoreExpr::BinaryOp {
            operator: "+".to_owned(),
            left: Box::new(field("pair", "left")),
            right: Box::new(field("pair", "right")),
        }),
    }
}

/// Builds one tuple pattern whose aggregate identity is never observed.
fn tuple_pattern_sum() -> CoreExpr {
    CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Tuple(vec![
                CorePattern::Var("left".to_owned()),
                CorePattern::Var("right".to_owned()),
            ]),
            value: CoreExpr::Tuple(vec![CoreExpr::Int(20), CoreExpr::Int(22)]),
        }],
        body: Box::new(CoreExpr::BinaryOp {
            operator: "+".to_owned(),
            left: Box::new(CoreExpr::Var("left".to_owned())),
            right: Box::new(CoreExpr::Var("right".to_owned())),
        }),
    }
}

/// Builds a fixed tuple local consumed only by a later destructuring binding.
fn local_tuple_pattern_sum() -> CoreExpr {
    CoreExpr::Let {
        bindings: vec![
            CoreLetBinding {
                pattern: CorePattern::Var("pair".to_owned()),
                value: CoreExpr::Tuple(vec![CoreExpr::Int(20), CoreExpr::Int(22)]),
            },
            CoreLetBinding {
                pattern: CorePattern::Tuple(vec![
                    CorePattern::Var("left".to_owned()),
                    CorePattern::Var("right".to_owned()),
                ]),
                value: CoreExpr::Var("pair".to_owned()),
            },
        ],
        body: Box::new(CoreExpr::BinaryOp {
            operator: "+".to_owned(),
            left: Box::new(CoreExpr::Var("left".to_owned())),
            right: Box::new(CoreExpr::Var("right".to_owned())),
        }),
    }
}

/// Replaces a live constructor with ordered scalar field locals.
#[test]
fn direct_field_uses_replace_the_live_constructor() {
    assert_eq!(
        scalar_replace_fixed_aggregates(&projected_sum(), &layouts()),
        CoreExpr::Let {
            bindings: vec![
                CoreLetBinding {
                    pattern: CorePattern::Var("$native_sroa_0_0".to_owned()),
                    value: CoreExpr::Int(20),
                },
                CoreLetBinding {
                    pattern: CorePattern::Var("$native_sroa_0_1".to_owned()),
                    value: CoreExpr::Int(22),
                },
            ],
            body: Box::new(CoreExpr::BinaryOp {
                operator: "+".to_owned(),
                left: Box::new(CoreExpr::Var("$native_sroa_0_0".to_owned())),
                right: Box::new(CoreExpr::Var("$native_sroa_0_1".to_owned())),
            }),
        }
    );
}

/// Preserves a constructor whenever its aggregate identity escapes.
#[test]
fn direct_aggregate_use_blocks_scalar_replacement() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("pair".to_owned()),
            value: pair(CoreExpr::Int(1), CoreExpr::Int(2)),
        }],
        body: Box::new(CoreExpr::Var("pair".to_owned())),
    };

    assert_eq!(
        scalar_replace_fixed_aggregates(&expression, &layouts()),
        expression
    );
}

/// Preserves allocation when a projection does not exist in the descriptor.
#[test]
fn unknown_projection_blocks_scalar_replacement() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("pair".to_owned()),
            value: pair(CoreExpr::Int(1), CoreExpr::Int(2)),
        }],
        body: Box::new(field("pair", "missing")),
    };

    assert_eq!(
        scalar_replace_fixed_aggregates(&expression, &layouts()),
        expression
    );
}

/// Does not rewrite a projection belonging to a later shadowing binding.
#[test]
fn lexical_shadowing_stops_projection_substitution() {
    let expression = CoreExpr::Let {
        bindings: vec![
            CoreLetBinding {
                pattern: CorePattern::Var("pair".to_owned()),
                value: pair(CoreExpr::Int(1), CoreExpr::Int(2)),
            },
            CoreLetBinding {
                pattern: CorePattern::Var("pair".to_owned()),
                value: pair(CoreExpr::Int(3), CoreExpr::Int(4)),
            },
        ],
        body: Box::new(field("pair", "right")),
    };
    let replaced = scalar_replace_fixed_aggregates(&expression, &layouts());
    let CoreExpr::Let { bindings, body } = replaced else {
        panic!("expected rewritten let");
    };

    assert!(matches!(
        bindings[0].value,
        CoreExpr::ConstructorCall { .. }
    ));
    assert_eq!(bindings.len(), 3);
    assert_eq!(*body, CoreExpr::Var("$native_sroa_0_1".to_owned()));
}

/// Keeps every field computation once and in constructor argument order.
#[test]
fn projected_fields_preserve_effectful_evaluation_once() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("pair".to_owned()),
            value: pair(
                CoreExpr::Call {
                    function: "left_value".to_owned(),
                    args: Vec::new(),
                },
                CoreExpr::Call {
                    function: "right_value".to_owned(),
                    args: Vec::new(),
                },
            ),
        }],
        body: Box::new(field("pair", "left")),
    };
    let CoreExpr::Let { bindings, body } = scalar_replace_fixed_aggregates(&expression, &layouts())
    else {
        panic!("expected rewritten let");
    };

    assert!(matches!(
        bindings.as_slice(),
        [
            CoreLetBinding { value: CoreExpr::Call { function: left, .. }, .. },
            CoreLetBinding { value: CoreExpr::Call { function: right, .. }, .. }
        ] if left == "left_value" && right == "right_value"
    ));
    assert_eq!(*body, CoreExpr::Var("$native_sroa_0_0".to_owned()));
}

/// Flattens a known tuple pattern into ordered scalar bindings.
#[test]
fn tuple_pattern_replaces_the_fixed_aggregate() {
    assert_eq!(
        scalar_replace_fixed_aggregates(&tuple_pattern_sum(), &layouts()),
        CoreExpr::Let {
            bindings: vec![
                CoreLetBinding {
                    pattern: CorePattern::Var("left".to_owned()),
                    value: CoreExpr::Int(20),
                },
                CoreLetBinding {
                    pattern: CorePattern::Var("right".to_owned()),
                    value: CoreExpr::Int(22),
                },
            ],
            body: Box::new(CoreExpr::BinaryOp {
                operator: "+".to_owned(),
                left: Box::new(CoreExpr::Var("left".to_owned())),
                right: Box::new(CoreExpr::Var("right".to_owned())),
            }),
        }
    );
}

/// Recursively flattens nested fixed shapes without constructing either tuple.
#[test]
fn nested_tuple_pattern_replaces_every_fixed_layer() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Tuple(vec![
                CorePattern::Tuple(vec![
                    CorePattern::Var("left".to_owned()),
                    CorePattern::Wildcard,
                ]),
                CorePattern::Var("right".to_owned()),
            ]),
            value: CoreExpr::Tuple(vec![
                CoreExpr::Tuple(vec![CoreExpr::Int(20), CoreExpr::Int(0)]),
                CoreExpr::Int(22),
            ]),
        }],
        body: Box::new(CoreExpr::Var("right".to_owned())),
    };
    let CoreExpr::Let { bindings, .. } = scalar_replace_fixed_aggregates(&expression, &layouts())
    else {
        panic!("expected rewritten nested tuple let");
    };

    assert_eq!(bindings.len(), 3);
    assert!(bindings
        .iter()
        .all(|binding| !matches!(binding.value, CoreExpr::Tuple(_))));
}

/// Removes an empty fixed tuple whose known match has no values to evaluate.
#[test]
fn empty_tuple_pattern_removes_the_empty_aggregate() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Tuple(Vec::new()),
            value: CoreExpr::Tuple(Vec::new()),
        }],
        body: Box::new(CoreExpr::Int(42)),
    };

    assert_eq!(
        scalar_replace_fixed_aggregates(&expression, &layouts()),
        CoreExpr::Let {
            bindings: Vec::new(),
            body: Box::new(CoreExpr::Int(42)),
        }
    );
}

/// Splits a fixed local at its producer while preserving consumer scope.
#[test]
fn later_tuple_destructuring_replaces_the_local_aggregate() {
    assert_eq!(
        scalar_replace_fixed_aggregates(&local_tuple_pattern_sum(), &layouts()),
        CoreExpr::Let {
            bindings: vec![
                CoreLetBinding {
                    pattern: CorePattern::Var("$native_sroa_local_0_0".to_owned()),
                    value: CoreExpr::Int(20),
                },
                CoreLetBinding {
                    pattern: CorePattern::Var("$native_sroa_local_0_1".to_owned()),
                    value: CoreExpr::Int(22),
                },
                CoreLetBinding {
                    pattern: CorePattern::Var("left".to_owned()),
                    value: CoreExpr::Var("$native_sroa_local_0_0".to_owned()),
                },
                CoreLetBinding {
                    pattern: CorePattern::Var("right".to_owned()),
                    value: CoreExpr::Var("$native_sroa_local_0_1".to_owned()),
                },
            ],
            body: Box::new(CoreExpr::BinaryOp {
                operator: "+".to_owned(),
                left: Box::new(CoreExpr::Var("left".to_owned())),
                right: Box::new(CoreExpr::Var("right".to_owned())),
            }),
        }
    );
}

/// Splits an exact constructor local at its later pattern consumer.
#[test]
fn later_constructor_destructuring_replaces_the_local_aggregate() {
    let expression = CoreExpr::Let {
        bindings: vec![
            CoreLetBinding {
                pattern: CorePattern::Var("pair".to_owned()),
                value: pair(CoreExpr::Int(20), CoreExpr::Int(22)),
            },
            CoreLetBinding {
                pattern: CorePattern::Constructor {
                    name: "Pair".to_owned(),
                    constructor_identity: Some("projection.Pair".to_owned()),
                    args: vec![
                        CorePattern::Var("left".to_owned()),
                        CorePattern::Var("right".to_owned()),
                    ],
                },
                value: CoreExpr::Var("pair".to_owned()),
            },
        ],
        body: Box::new(CoreExpr::Var("right".to_owned())),
    };
    let CoreExpr::Let { bindings, .. } = scalar_replace_fixed_aggregates(&expression, &layouts())
    else {
        panic!("expected rewritten local constructor let");
    };

    assert_eq!(bindings.len(), 4);
    assert!(bindings
        .iter()
        .all(|binding| !matches!(binding.value, CoreExpr::ConstructorCall { .. })));
}

/// Keeps a local aggregate when code observes it before destructuring.
#[test]
fn aggregate_use_before_destructuring_blocks_local_replacement() {
    let mut expression = local_tuple_pattern_sum();
    let CoreExpr::Let { bindings, .. } = &mut expression else {
        panic!("expected local tuple let");
    };
    bindings.insert(
        1,
        CoreLetBinding {
            pattern: CorePattern::Var("observed".to_owned()),
            value: CoreExpr::Call {
                function: "observe".to_owned(),
                args: vec![CoreExpr::Var("pair".to_owned())],
            },
        },
    );

    assert_eq!(
        scalar_replace_fixed_aggregates(&expression, &layouts()),
        expression
    );
}

/// Keeps a local aggregate when its identity remains live after destructuring.
#[test]
fn aggregate_use_after_destructuring_blocks_local_replacement() {
    let mut expression = local_tuple_pattern_sum();
    let CoreExpr::Let { body, .. } = &mut expression else {
        panic!("expected local tuple let");
    };
    *body = Box::new(CoreExpr::Var("pair".to_owned()));

    assert_eq!(
        scalar_replace_fixed_aggregates(&expression, &layouts()),
        expression
    );
}

/// Preserves wildcard field evaluation while removing its tuple container.
#[test]
fn tuple_wildcard_keeps_source_evaluation_order() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Tuple(vec![
                CorePattern::Wildcard,
                CorePattern::Var("right".to_owned()),
            ]),
            value: CoreExpr::Tuple(vec![
                CoreExpr::Call {
                    function: "observe_left".to_owned(),
                    args: Vec::new(),
                },
                CoreExpr::Int(22),
            ]),
        }],
        body: Box::new(CoreExpr::Var("right".to_owned())),
    };
    let CoreExpr::Let { bindings, .. } = scalar_replace_fixed_aggregates(&expression, &layouts())
    else {
        panic!("expected rewritten tuple let");
    };

    assert!(matches!(
        bindings.as_slice(),
        [
            CoreLetBinding {
                pattern: CorePattern::Var(hidden),
                value: CoreExpr::Call { function, .. },
            },
            CoreLetBinding {
                pattern: CorePattern::Var(right),
                value: CoreExpr::Int(22),
            },
        ] if hidden == "$native_sroa_pattern_0_0"
            && function == "observe_left"
            && right == "right"
    ));
}

/// Flattens a constructor pattern only when its resolved identity is exact.
#[test]
fn matching_constructor_pattern_replaces_the_fixed_aggregate() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Constructor {
                name: "Pair".to_owned(),
                constructor_identity: Some("projection.Pair".to_owned()),
                args: vec![
                    CorePattern::Var("left".to_owned()),
                    CorePattern::Var("right".to_owned()),
                ],
            },
            value: pair(CoreExpr::Int(20), CoreExpr::Int(22)),
        }],
        body: Box::new(CoreExpr::Var("right".to_owned())),
    };
    let CoreExpr::Let { bindings, .. } = scalar_replace_fixed_aggregates(&expression, &layouts())
    else {
        panic!("expected rewritten constructor let");
    };

    assert_eq!(bindings.len(), 2);
    assert!(bindings
        .iter()
        .all(|binding| matches!(binding.pattern, CorePattern::Var(_))));
}

/// Leaves refutable fixed patterns under the ordinary matcher contract.
#[test]
fn refutable_tuple_pattern_is_not_scalar_replaced() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Tuple(vec![CorePattern::Int(20), CorePattern::Wildcard]),
            value: CoreExpr::Tuple(vec![CoreExpr::Int(20), CoreExpr::Int(22)]),
        }],
        body: Box::new(CoreExpr::Int(42)),
    };

    assert_eq!(
        scalar_replace_fixed_aggregates(&expression, &layouts()),
        expression
    );
}

/// Makes a projection-only function eligible for the scalar native profile.
#[test]
fn scalar_replacement_participates_in_production_admission() {
    assert!(is_scalar_candidate(&function(projected_sum()), &layouts()));
}

/// Emits allocation-free NativeIR for a live projection-only aggregate.
#[test]
fn production_lowering_emits_only_scalar_field_locals() {
    let mut stable_ids = std::collections::HashSet::new();
    let (native, continuations) = lower_native_function(
        "projection",
        &function(projected_sum()),
        &HashMap::new(),
        &HashMap::new(),
        &layouts(),
        &std::collections::HashSet::new(),
        &HashMap::new(),
        &mut stable_ids,
    )
    .expect("projection lowering");

    assert!(matches!(
        native.body,
        NativeExpr::Let { ref bindings, ref body }
            if bindings == &[NativeExpr::Int(20), NativeExpr::Int(22)]
                && matches!(body.as_ref(), NativeExpr::Binary {
                    operator: NativeBinaryOperator::Add,
                    left,
                    right,
                    ..
                } if left.as_ref() == &NativeExpr::Param(0)
                    && right.as_ref() == &NativeExpr::Param(1))
    ));
    assert!(continuations.is_empty());
}

/// Carries source constructors through the real application admission path.
#[test]
fn source_application_lowering_scalar_replaces_projected_constructor() {
    let syntax = parse_module_as_syntax_output(
        "\
module projection.\n\
\n\
pub struct Pair {\n\
    left: Int,\n\
    right: Int\n\
}.\n\
\n\
pub constructor Pair {\n\
    (left: Int, right: Int): Pair ->\n\
        Pair {left: left, right: right}\n\
}.\n\
\n\
pub projected(): Int ->\n\
    let pair = Pair(20, 22);\n\
    pair.left + pair.right.\n",
    )
    .expect("projection source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("native application");
    let function = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "projected")
        .expect("projected native function");

    assert!(
        matches!(
            function.body,
            NativeExpr::Let { ref bindings, ref body }
                if bindings == &[NativeExpr::Int(20), NativeExpr::Int(22)]
                    && matches!(body.as_ref(), NativeExpr::Binary {
                        operator: NativeBinaryOperator::Add,
                        ..
                    })
        ),
        "body={:?}",
        function.body
    );
}

/// Carries tuple destructuring through source checking and application lowering.
#[test]
fn source_application_lowering_scalar_replaces_tuple_pattern() {
    let syntax = parse_module_as_syntax_output(
        "\
module tuple_pattern.\n\
\n\
pub tuple_sum(): Int ->\n\
    let {left, right} = {20, 22};\n\
    left + right.\n",
    )
    .expect("tuple-pattern source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("native application");
    let function = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "tuple_sum")
        .expect("tuple_sum native function");

    assert!(
        matches!(
            function.body,
            NativeExpr::Let { ref bindings, ref body }
                if bindings == &[NativeExpr::Int(20), NativeExpr::Int(22)]
                    && matches!(body.as_ref(), NativeExpr::Binary {
                        operator: NativeBinaryOperator::Add,
                        ..
                    })
        ),
        "body={:?}",
        function.body
    );
}

/// Carries a later local destructuring through source and native lowering.
#[test]
fn source_application_lowering_scalar_replaces_local_tuple_pattern() {
    let syntax = parse_module_as_syntax_output(
        "\
module local_tuple_pattern.\n\
\n\
pub tuple_sum(): Int ->\n\
    let pair = {20, 22};\n\
    let {left, right} = pair;\n\
    left + right.\n",
    )
    .expect("local tuple-pattern source");
    let resolved = resolve_syntax_module_output(&syntax).module;
    let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:#?}");
    let core = lower_syntax_module_output_to_core(&syntax, &resolved);
    let modules = NativeModule::lower_application(&[&core]).expect("native application");
    let function = modules
        .iter()
        .flat_map(|module| &module.functions)
        .find(|function| function.name == "tuple_sum")
        .expect("tuple_sum native function");

    assert!(
        matches!(
            function.body,
            NativeExpr::Let { ref bindings, ref body }
                if bindings == &[
                    NativeExpr::Int(20),
                    NativeExpr::Int(22),
                    NativeExpr::Param(0),
                    NativeExpr::Param(1),
                ] && matches!(body.as_ref(), NativeExpr::Binary {
                    operator: NativeBinaryOperator::Add,
                    left,
                    right,
                    ..
                } if left.as_ref() == &NativeExpr::Param(2)
                    && right.as_ref() == &NativeExpr::Param(3))
        ),
        "body={:?}",
        function.body
    );
}

/// Executes tuple-pattern machine code without a managed allocator callback.
#[test]
fn generated_tuple_pattern_path_has_no_allocator_reachability() {
    let mut stable_ids = std::collections::HashSet::new();
    let (native, continuations) = lower_native_function(
        "tuple_pattern",
        &function(tuple_pattern_sum()),
        &HashMap::new(),
        &HashMap::new(),
        &layouts(),
        &std::collections::HashSet::new(),
        &HashMap::new(),
        &mut stable_ids,
    )
    .expect("tuple-pattern function");
    let export_id = native.export_id;
    let object = emit_native_application_object(
        "tuple_pattern_scalar_replacement",
        &[NativeModule {
            name: "tuple_pattern".to_owned(),
            functions: vec![native],
            continuations,
            managed_layouts: vec![],
            managed_collections: vec![],
            atoms: vec![],
        }],
    )
    .expect("tuple-pattern object");
    let root = std::env::temp_dir().join(format!(
        "terlan-tuple-pattern-sroa-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("tuple-pattern fixture directory");
    let object_path = root.join("tuple-pattern.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("tuple-pattern object bytes");
    fs::write(
        &harness_path,
        NULL_ALLOCATOR_HARNESS.replace("$EXPORT_ID", &export_id.to_string()),
    )
    .expect("tuple-pattern harness");

    let compile = Command::new("rustc")
        .arg("--edition=2021")
        .arg(&harness_path)
        .arg("-C")
        .arg(format!("link-arg={}", object_path.display()))
        .arg("-o")
        .arg(&executable_path)
        .output()
        .expect("compile tuple-pattern harness");
    assert!(
        compile.status.success(),
        "tuple-pattern harness failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable_path)
        .output()
        .expect("run tuple-pattern harness");
    assert!(
        run.status.success(),
        "tuple-pattern object required an allocator:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    fs::remove_dir_all(root).expect("remove tuple-pattern fixture");
}

/// Executes projection-only machine code without a managed allocator callback.
#[test]
fn generated_projection_path_has_no_allocator_reachability() {
    let mut stable_ids = std::collections::HashSet::new();
    let (native, continuations) = lower_native_function(
        "projection",
        &function(projected_sum()),
        &HashMap::new(),
        &HashMap::new(),
        &layouts(),
        &std::collections::HashSet::new(),
        &HashMap::new(),
        &mut stable_ids,
    )
    .expect("projection function");
    let export_id = native.export_id;
    let object = emit_native_application_object(
        "scalar_replacement",
        &[NativeModule {
            name: "projection".to_owned(),
            functions: vec![native],
            continuations,
            managed_layouts: vec![],
            managed_collections: vec![],
            atoms: vec![],
        }],
    )
    .expect("projection object");
    let root = std::env::temp_dir().join(format!(
        "terlan-managed-sroa-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("projection fixture directory");
    let object_path = root.join("projection.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("projection object bytes");
    fs::write(
        &harness_path,
        NULL_ALLOCATOR_HARNESS.replace("$EXPORT_ID", &export_id.to_string()),
    )
    .expect("projection harness");

    let compile = Command::new("rustc")
        .arg("--edition=2021")
        .arg(&harness_path)
        .arg("-C")
        .arg(format!("link-arg={}", object_path.display()))
        .arg("-o")
        .arg(&executable_path)
        .output()
        .expect("compile projection harness");
    assert!(
        compile.status.success(),
        "projection harness failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable_path)
        .output()
        .expect("run projection harness");
    assert!(
        run.status.success(),
        "projection object required an allocator:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    fs::remove_dir_all(root).expect("remove projection fixture");
}

/// Linked probe that supplies no managed allocator to generated code.
const NULL_ALLOCATOR_HARNESS: &str = r#"
use std::ffi::c_void;

unsafe extern "C" {
    fn terlan_native_dispatch_v2(
        context: *mut c_void,
        allocator: *const c_void,
        closure_resolver: *const c_void,
        export_id: u64,
        arguments: *const i64,
        arity: u64,
        result: *mut i64,
        transitions: *mut i64,
        transition_capacity: u64,
        transition_len: *mut u64,
    ) -> i32;
}

fn main() {
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v2(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            $EXPORT_ID,
            std::ptr::null(),
            0,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, 42);
    assert_eq!(transition_len, 0);
}
"#;
