use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::terlan_typeck::{
    CoreConstructorDecl, CoreEffectSet, CoreExpr, CoreExprSummary, CoreFunction,
    CoreFunctionClause, CoreIntrinsicCall, CoreIntrinsicId, CoreLetBinding, CoreParam, CorePattern,
    CorePrimitiveIntrinsic, CoreProofCoverage, CoreType,
};

use super::constructors::{native_constructor_layouts, NativeConstructorLayouts};
use super::escape::retained_managed_bindings;
use super::expression::lower_expr_with_constructors;
use super::native_object_test_support::with_dispatch_lookup_harness;
use super::{
    call_composition::rebase_callee_locals, emit_native_application_object,
    expr_calls_are_supported, lower_native_function, NativeExpr, NativeModule, NativeType,
};

/// Builds the fixed constructor declarations used by escape regressions.
fn layouts() -> NativeConstructorLayouts {
    let result = CoreType::Apply {
        constructor: "Result".to_owned(),
        args: vec![CoreType::Int, CoreType::Int],
    };
    let option_result = CoreType::Apply {
        constructor: "Option".to_owned(),
        args: vec![result.clone()],
    };
    let declarations = vec![
        CoreConstructorDecl {
            name: "Ok".to_owned(),
            public: true,
            min_arity: 1,
            params: vec![CoreParam {
                name: "value".to_owned(),
                ty: "Int".to_owned(),
                core_ty: Some(CoreType::Int),
            }],
            vararg: None,
            return_type: "Result[Int, Int]".to_owned(),
            core_return_type: Some(result.clone()),
        },
        CoreConstructorDecl {
            name: "Error".to_owned(),
            public: true,
            min_arity: 1,
            params: vec![CoreParam {
                name: "reason".to_owned(),
                ty: "Int".to_owned(),
                core_ty: Some(CoreType::Int),
            }],
            vararg: None,
            return_type: "Result[Int, Int]".to_owned(),
            core_return_type: Some(result.clone()),
        },
        CoreConstructorDecl {
            name: "Some".to_owned(),
            public: true,
            min_arity: 1,
            params: vec![CoreParam {
                name: "value".to_owned(),
                ty: "Result[Int, Int]".to_owned(),
                core_ty: Some(result),
            }],
            vararg: None,
            return_type: "Option[Result[Int, Int]]".to_owned(),
            core_return_type: Some(option_result),
        },
    ];
    native_constructor_layouts(&[("escape", declarations.as_slice())], "escape")
        .expect("escape constructor layouts")
}

/// Creates one fully resolved constructor call.
fn constructor(name: &str, argument: CoreExpr) -> CoreExpr {
    CoreExpr::ConstructorCall {
        constructor: name.to_owned(),
        constructor_identity: Some(format!("escape.{name}")),
        args: vec![argument],
    }
}

/// Lowers one expression against the escape constructor table.
fn lower(expr: &CoreExpr) -> Result<NativeExpr, String> {
    lower_expr_with_constructors(
        expr,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new(),
        &layouts(),
    )
}

/// Wraps one body in the production native-function input contract.
fn function(body: CoreExpr, return_type: &str, core_return_type: CoreType) -> CoreFunction {
    CoreFunction {
        name: "optimized".to_owned(),
        arity: 0,
        public: true,
        generic_params: Vec::new(),
        native_operation: None,
        params: Vec::new(),
        return_type: return_type.to_owned(),
        core_return_type: Some(core_return_type),
        clauses: vec![CoreFunctionClause {
            patterns: Vec::new(),
            core_patterns: Vec::new(),
            pattern_proof_coverage: Vec::new(),
            pattern_checked_preservation_evidence: Vec::new(),
            guard: None,
            body: CoreExprSummary {
                kind: "escape-test".to_owned(),
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

/// Removes an unused constructor before NativeIR can inventory an allocation.
#[test]
fn dead_allocation_only_constructor_is_eliminated() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("discarded".to_owned()),
            value: constructor("Ok", CoreExpr::Int(41)),
        }],
        body: Box::new(CoreExpr::Int(7)),
    };

    assert_eq!(
        lower(&expression).expect("dead constructor lowering"),
        NativeExpr::Int(7)
    );
}

/// Retains a constructor whose managed result reaches the function result.
#[test]
fn live_constructor_remains_allocated() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("message".to_owned()),
            value: constructor("Ok", CoreExpr::Int(41)),
        }],
        body: Box::new(CoreExpr::Var("message".to_owned())),
    };

    let NativeExpr::Let { bindings, body } = lower(&expression).expect("live constructor lowering")
    else {
        panic!("live constructor must remain in a let region");
    };
    assert!(matches!(
        bindings.as_slice(),
        [NativeExpr::Construct { .. }]
    ));
    assert_eq!(*body, NativeExpr::Param(0));
}

/// Removes an entire dead constructor chain rather than preserving inner roots.
#[test]
fn dead_nested_constructor_chain_is_eliminated_transitively() {
    let bindings = vec![
        CoreLetBinding {
            pattern: CorePattern::Var("inner".to_owned()),
            value: constructor("Ok", CoreExpr::Int(1)),
        },
        CoreLetBinding {
            pattern: CorePattern::Var("outer".to_owned()),
            value: constructor("Some", CoreExpr::Var("inner".to_owned())),
        },
    ];
    let expression = CoreExpr::Let {
        bindings: bindings.clone(),
        body: Box::new(CoreExpr::Int(9)),
    };

    assert_eq!(
        retained_managed_bindings(&bindings, &CoreExpr::Int(9)),
        [false, false]
    );
    assert_eq!(
        lower(&expression).expect("nested constructor lowering"),
        NativeExpr::Int(9)
    );
}

/// Preserves source evaluation when a constructor field invokes unknown code.
#[test]
fn constructor_with_unproven_field_effect_is_not_eliminated() {
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("discarded".to_owned()),
            value: constructor(
                "Ok",
                CoreExpr::Call {
                    function: "observe".to_owned(),
                    args: Vec::new(),
                },
            ),
        }],
        body: Box::new(CoreExpr::Int(7)),
    };
    let functions = HashMap::from([(("observe".to_owned(), 0), 0)]);
    let function_types = HashMap::from([(("observe".to_owned(), 0), NativeType::Int)]);
    let lowered = lower_expr_with_constructors(
        &expression,
        &HashMap::new(),
        &HashMap::new(),
        &functions,
        &function_types,
        &layouts(),
    )
    .expect("effect-preserving constructor lowering");

    let NativeExpr::Let { bindings, .. } = lowered else {
        panic!("unproven field effect must retain its constructor");
    };
    assert!(matches!(
        bindings.as_slice(),
        [NativeExpr::Construct { fields, .. }]
            if matches!(fields.as_slice(), [NativeExpr::Call { function: 0, args }] if args.is_empty())
    ));
}

/// Eliminates a shadowed constructor while retaining the visible replacement.
#[test]
fn lexical_shadowing_does_not_keep_the_hidden_constructor_live() {
    let expression = CoreExpr::Let {
        bindings: vec![
            CoreLetBinding {
                pattern: CorePattern::Var("value".to_owned()),
                value: constructor("Ok", CoreExpr::Int(1)),
            },
            CoreLetBinding {
                pattern: CorePattern::Var("value".to_owned()),
                value: constructor("Ok", CoreExpr::Int(2)),
            },
        ],
        body: Box::new(CoreExpr::Var("value".to_owned())),
    };

    let NativeExpr::Let { bindings, body } = lower(&expression).expect("shadow lowering") else {
        panic!("visible constructor must remain");
    };
    assert_eq!(bindings.len(), 1);
    assert!(matches!(bindings[0], NativeExpr::Construct { .. }));
    assert_eq!(*body, NativeExpr::Param(0));
}

/// Reindexes retained scalar locals after an earlier constructor disappears.
#[test]
fn retained_local_indexes_close_over_eliminated_allocations() {
    let expression = CoreExpr::Let {
        bindings: vec![
            CoreLetBinding {
                pattern: CorePattern::Var("discarded".to_owned()),
                value: constructor("Ok", CoreExpr::Int(1)),
            },
            CoreLetBinding {
                pattern: CorePattern::Var("answer".to_owned()),
                value: CoreExpr::Int(42),
            },
        ],
        body: Box::new(CoreExpr::Var("answer".to_owned())),
    };

    assert_eq!(
        lower(&expression).expect("local reindex lowering"),
        NativeExpr::Let {
            bindings: vec![NativeExpr::Int(42)],
            body: Box::new(NativeExpr::Param(0)),
        }
    );
}

/// Proves production function lowering consumes layouts and emits no dead allocation.
#[test]
fn production_function_lowering_eliminates_allocator_reachability() {
    let body = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("discarded".to_owned()),
            value: constructor("Ok", CoreExpr::Int(41)),
        }],
        body: Box::new(CoreExpr::Int(7)),
    };
    let mut stable_ids = std::collections::HashSet::new();

    let (native, continuations) = lower_native_function(
        "escape",
        &function(body, "Int", CoreType::Int),
        &layouts(),
        &mut stable_ids,
    )
    .expect("production native function lowering");

    assert_eq!(native.body, NativeExpr::Int(7));
    assert!(continuations.is_empty());
}

/// Carries a live constructed value through production suspension lowering.
#[test]
fn production_suspension_lowering_uses_the_same_constructor_layouts() {
    let result_type = CoreType::Apply {
        constructor: "Result".to_owned(),
        args: vec![CoreType::Int, CoreType::Int],
    };
    let body = CoreExpr::Let {
        bindings: vec![
            CoreLetBinding {
                pattern: CorePattern::Var("message".to_owned()),
                value: constructor("Ok", CoreExpr::Int(41)),
            },
            CoreLetBinding {
                pattern: CorePattern::Var("resumed".to_owned()),
                value: CoreExpr::Intrinsic(CoreIntrinsicCall {
                    id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmProcessYield),
                    args: Vec::new(),
                    return_type: CoreType::Named("Unit".to_owned()),
                    effects: CoreEffectSet {
                        effects: vec!["vm.process".to_owned()],
                    },
                    span: crate::terlan_syntax::span::Span::new(0, 0),
                }),
            },
        ],
        body: Box::new(CoreExpr::Var("message".to_owned())),
    };
    let mut stable_ids = std::collections::HashSet::new();

    let (native, continuations) = lower_native_function(
        "escape",
        &function(body, "Result[Int, Int]", result_type),
        &layouts(),
        &mut stable_ids,
    )
    .expect("suspending constructor lowering");

    assert!(matches!(
        native.body,
        NativeExpr::Let { ref bindings, ref body }
            if matches!(bindings.as_slice(), [NativeExpr::Construct { .. }])
                && matches!(body.as_ref(), NativeExpr::Suspend { values, .. }
                    if matches!(values.as_slice(), [NativeExpr::Param(0)]))
    ));
    assert_eq!(continuations.len(), 1);
    assert!(matches!(
        continuations[0].params.as_slice(),
        [NativeType::ManagedRef(_)]
    ));
    assert_eq!(continuations[0].body, NativeExpr::Param(0));
}

/// Rejects a constructor field that conceals a non-tail suspending call.
#[test]
fn constructor_fields_cannot_hide_suspending_calls_from_admission() {
    let expression = constructor(
        "Ok",
        CoreExpr::Call {
            function: "pause".to_owned(),
            args: Vec::new(),
        },
    );
    let identities = [("pause", 0)];
    let suspending = std::collections::HashSet::from([("pause".to_owned(), 0)]);

    assert!(!expr_calls_are_supported(
        &expression,
        &identities,
        &suspending,
        &std::collections::HashSet::new(),
        true,
    ));
}

/// Rebases managed constructor fields when a callee continuation is composed.
#[test]
fn composed_continuations_rebase_constructor_field_locals() {
    let NativeExpr::Construct {
        descriptor,
        encoded_layout,
        ..
    } = lower(&constructor("Ok", CoreExpr::Int(1))).expect("constructor lowering")
    else {
        panic!("expected constructor expression");
    };
    let body = NativeExpr::Construct {
        descriptor,
        encoded_layout,
        fields: vec![NativeExpr::Param(2)],
    };

    assert!(matches!(
        rebase_callee_locals(&body, 1, 3),
        NativeExpr::Construct { fields, .. }
            if fields == vec![NativeExpr::Param(5)]
    ));
}

/// Executes optimized machine code successfully without providing an allocator.
#[test]
fn generated_dead_constructor_path_has_no_allocator_reachability() {
    let body = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("discarded".to_owned()),
            value: constructor("Ok", CoreExpr::Int(41)),
        }],
        body: Box::new(CoreExpr::Int(7)),
    };
    let mut stable_ids = std::collections::HashSet::new();
    let (native, continuations) = lower_native_function(
        "escape",
        &function(body, "Int", CoreType::Int),
        &layouts(),
        &mut stable_ids,
    )
    .expect("optimized function");
    let export_id = native.export_id;
    let object = emit_native_application_object(
        "escape_elimination",
        &[NativeModule {
            name: "escape".to_owned(),
            functions: vec![native],
            continuations,
            managed_layouts: vec![],
            managed_collections: vec![],
            atoms: vec![],
        }],
    )
    .expect("optimized native object");
    let root = std::env::temp_dir().join(format!(
        "terlan-managed-escape-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("escape fixture directory");
    let object_path = root.join("escape.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("optimized object");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(ALLOCATION_FREE_HARNESS)
            .replace("$EXPORT_ID", &export_id.to_string()),
    )
    .expect("allocation-free harness");

    let compile = Command::new("rustc")
        .arg("--edition=2021")
        .arg(&harness_path)
        .arg("-C")
        .arg(format!("link-arg={}", object_path.display()))
        .arg("-o")
        .arg(&executable_path)
        .output()
        .expect("compile allocation-free harness");
    assert!(
        compile.status.success(),
        "allocation-free harness failed:\n{}",
        String::from_utf8_lossy(&compile.stderr)
    );
    let run = Command::new(&executable_path)
        .output()
        .expect("run allocation-free harness");
    assert!(
        run.status.success(),
        "optimized object required an allocator:\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    fs::remove_dir_all(root).expect("remove escape fixture");
}

/// Linked probe that invokes optimized code with no managed allocator.
const ALLOCATION_FREE_HARNESS: &str = r#"
use std::ffi::c_void;

unsafe extern "C" {
    fn terlan_native_dispatch_v3(
        context: *mut c_void,
        allocator: *const c_void,
        closure_resolver: *const c_void,
        dispatch_lookup: *const c_void,
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
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
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
    assert_eq!(result, 7);
    assert_eq!(transition_len, 0);
}
"#;
