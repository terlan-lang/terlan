#[cfg(unix)]
use super::native_object_test_support::with_dispatch_lookup_harness;
use super::tail_position::{
    lower_recursive_tail_calls, mutual_tail_components, validate_recursive_tail_targets,
};
use super::{
    NativeBinaryOperator, NativeContinuation, NativeExpr, NativeFunction, NativeModule,
    NativeTransitionOperation, NativeType,
};
use crate::runtime::native_image::managed::SemanticTypeId;
use crate::runtime::native_image::managed::{
    encode_aggregate_layout, encode_collection_layout, encode_list_from_elements_operation,
    ManagedAggregateDescriptor, ManagedCollectionDescriptor, ManagedFieldType,
};
#[cfg(unix)]
use std::fs;
use std::sync::Arc;
#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(unix)]
#[path = "tail_position_test/support.rs"]
mod support;
#[cfg(unix)]
use support::{
    assert_component_relocation, assert_defined_component_has_no_recursive_relocation,
    assert_no_component_relocation, assert_no_self_relocation,
    compile_and_run_many_with_small_stack, compile_and_run_with_small_stack,
};

fn function(body: NativeExpr) -> NativeFunction {
    NativeFunction {
        export_id: 7,
        name: "loop".to_string(),
        public: true,
        arity: 2,
        source_module: "app.Tail".to_string(),
        source_function: "loop".to_string(),
        source_arity: 2,
        callable_captures: Vec::new(),
        params: vec![NativeType::Int, NativeType::Int],
        return_type: NativeType::Int,
        body,
    }
}

fn module(body: NativeExpr) -> NativeModule {
    NativeModule {
        name: "app.Tail".to_string(),
        functions: vec![function(body)],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

fn decrement() -> NativeExpr {
    NativeExpr::Binary {
        operator: NativeBinaryOperator::Subtract,
        operand_type: NativeType::Int,
        left: Box::new(NativeExpr::Param(0)),
        right: Box::new(NativeExpr::Int(1)),
    }
}

fn increment_accumulator() -> NativeExpr {
    NativeExpr::Binary {
        operator: NativeBinaryOperator::Add,
        operand_type: NativeType::Int,
        left: Box::new(NativeExpr::Param(1)),
        right: Box::new(NativeExpr::Int(1)),
    }
}

fn deep_countdown_body() -> NativeExpr {
    NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Equal,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(0)),
                    right: Box::new(NativeExpr::Int(0)),
                },
                NativeExpr::Param(1),
            ),
            (
                NativeExpr::Bool(true),
                NativeExpr::Call {
                    function: 0,
                    args: vec![decrement(), increment_accumulator()],
                },
            ),
        ],
    }
}

fn deep_let_countdown_body() -> NativeExpr {
    NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Equal,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(0)),
                    right: Box::new(NativeExpr::Int(0)),
                },
                NativeExpr::Param(1),
            ),
            (
                NativeExpr::Bool(true),
                NativeExpr::Let {
                    bindings: vec![increment_accumulator()],
                    body: Box::new(NativeExpr::Call {
                        function: 0,
                        args: vec![decrement(), NativeExpr::Param(2)],
                    }),
                },
            ),
        ],
    }
}

fn suspending_countdown_body() -> NativeExpr {
    NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Equal,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(0)),
                    right: Box::new(NativeExpr::Int(0)),
                },
                NativeExpr::Suspend {
                    operation: NativeTransitionOperation::Yield,
                    arguments: Vec::new(),
                    continuation_id: 91,
                    values: vec![NativeExpr::Param(1)],
                },
            ),
            (
                NativeExpr::Bool(true),
                NativeExpr::Call {
                    function: 0,
                    args: vec![decrement(), increment_accumulator()],
                },
            ),
        ],
    }
}

fn failing_countdown_body() -> NativeExpr {
    NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Equal,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(0)),
                    right: Box::new(NativeExpr::Int(0)),
                },
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Divide,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(1)),
                    right: Box::new(NativeExpr::Int(0)),
                },
            ),
            (
                NativeExpr::Bool(true),
                NativeExpr::Call {
                    function: 0,
                    args: vec![decrement(), increment_accumulator()],
                },
            ),
        ],
    }
}

fn non_tail_countdown_body() -> NativeExpr {
    NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Equal,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(0)),
                    right: Box::new(NativeExpr::Int(0)),
                },
                NativeExpr::Param(1),
            ),
            (
                NativeExpr::Bool(true),
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Add,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Call {
                        function: 0,
                        args: vec![decrement(), NativeExpr::Param(1)],
                    }),
                    right: Box::new(NativeExpr::Int(1)),
                },
            ),
        ],
    }
}

fn mutual_countdown_module() -> NativeModule {
    let body = |target| NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Equal,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(0)),
                    right: Box::new(NativeExpr::Int(0)),
                },
                NativeExpr::Param(1),
            ),
            (
                NativeExpr::Bool(true),
                NativeExpr::Call {
                    function: target,
                    args: vec![decrement(), increment_accumulator()],
                },
            ),
        ],
    };
    let mut first = function(body(1));
    first.name = "even".to_string();
    first.source_function = first.name.clone();
    let mut second = function(body(0));
    second.export_id = 8;
    second.name = "odd".to_string();
    second.source_function = second.name.clone();
    NativeModule {
        name: "app.Tail".to_string(),
        functions: vec![first, second],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

fn suspending_mutual_countdown_module() -> NativeModule {
    let body = |target| NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Equal,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(0)),
                    right: Box::new(NativeExpr::Int(0)),
                },
                NativeExpr::Suspend {
                    operation: NativeTransitionOperation::Yield,
                    arguments: Vec::new(),
                    continuation_id: 91,
                    values: vec![NativeExpr::Param(1)],
                },
            ),
            (
                NativeExpr::Bool(true),
                NativeExpr::Call {
                    function: target,
                    args: vec![decrement(), increment_accumulator()],
                },
            ),
        ],
    };
    let mut first = function(body(1));
    first.name = "even".to_string();
    first.source_function = first.name.clone();
    let mut second = function(body(0));
    second.export_id = 8;
    second.name = "odd".to_string();
    second.source_function = second.name.clone();
    NativeModule {
        name: "app.Tail".to_string(),
        functions: vec![first, second],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

fn cancelling_mutual_countdown_module() -> NativeModule {
    let body = |target| NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Equal,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(0)),
                    right: Box::new(NativeExpr::Int(0)),
                },
                NativeExpr::Suspend {
                    operation: NativeTransitionOperation::Cancellation,
                    arguments: vec![NativeExpr::Int(123)],
                    continuation_id: 93,
                    values: vec![NativeExpr::Param(1)],
                },
            ),
            (
                NativeExpr::Bool(true),
                NativeExpr::Call {
                    function: target,
                    args: vec![decrement(), increment_accumulator()],
                },
            ),
        ],
    };
    let mut first = function(body(1));
    first.name = "cancel_even".to_string();
    first.source_function = first.name.clone();
    let mut second = function(body(0));
    second.export_id = 8;
    second.name = "cancel_odd".to_string();
    second.source_function = second.name.clone();
    NativeModule {
        name: "app.Tail".to_string(),
        functions: vec![first, second],
        continuations: vec![NativeContinuation {
            id: 93,
            source_module: "app.Tail".to_string(),
            source_function: "cancel_even".to_string(),
            source_arity: 2,
            source_span: None,
            capture_names: Vec::new(),
            params: vec![NativeType::Int],
            return_type: NativeType::Int,
            body: NativeExpr::Binary {
                operator: NativeBinaryOperator::Add,
                operand_type: NativeType::Int,
                left: Box::new(NativeExpr::Param(0)),
                right: Box::new(NativeExpr::Int(1)),
            },
        }],
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

fn heterogeneous_mutual_countdown_module() -> NativeModule {
    let condition = || NativeExpr::Binary {
        operator: NativeBinaryOperator::Equal,
        operand_type: NativeType::Int,
        left: Box::new(NativeExpr::Param(0)),
        right: Box::new(NativeExpr::Int(0)),
    };
    let mut narrow = function(NativeExpr::If {
        clauses: vec![
            (condition(), NativeExpr::Int(42)),
            (
                NativeExpr::Bool(true),
                NativeExpr::Call {
                    function: 1,
                    args: vec![decrement(), NativeExpr::Int(0)],
                },
            ),
        ],
    });
    narrow.name = "narrow".to_string();
    narrow.source_function = narrow.name.clone();
    narrow.arity = 1;
    narrow.params = vec![NativeType::Int];
    let mut wide = function(NativeExpr::If {
        clauses: vec![
            (condition(), NativeExpr::Int(42)),
            (
                NativeExpr::Bool(true),
                NativeExpr::Call {
                    function: 0,
                    args: vec![decrement()],
                },
            ),
        ],
    });
    wide.export_id = 8;
    wide.name = "wide".to_string();
    wide.source_function = wide.name.clone();
    NativeModule {
        name: "app.Tail".to_string(),
        functions: vec![narrow, wide],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

fn managed_mutual_countdown_module() -> NativeModule {
    let managed = NativeType::ManagedRef(
        SemanticTypeId::from_canonical("TailToken").expect("managed tail token identity"),
    );
    let body = |target| NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Equal,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(0)),
                    right: Box::new(NativeExpr::Int(0)),
                },
                NativeExpr::Param(1),
            ),
            (
                NativeExpr::Bool(true),
                NativeExpr::Call {
                    function: target,
                    args: vec![decrement(), NativeExpr::Param(1)],
                },
            ),
        ],
    };
    let mut first = function(body(1));
    first.name = "managed_even".to_string();
    first.source_function = first.name.clone();
    first.params = vec![NativeType::Int, managed];
    first.return_type = managed;
    let mut second = function(body(0));
    second.export_id = 8;
    second.name = "managed_odd".to_string();
    second.source_function = second.name.clone();
    second.params = vec![NativeType::Int, managed];
    second.return_type = managed;
    NativeModule {
        name: "app.Tail".to_string(),
        functions: vec![first, second],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

fn managed_parallel_swap_module() -> NativeModule {
    let managed = NativeType::ManagedRef(
        SemanticTypeId::from_canonical("TailToken").expect("managed tail token identity"),
    );
    let mut swap = function(NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Equal,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(0)),
                    right: Box::new(NativeExpr::Int(0)),
                },
                NativeExpr::Param(2),
            ),
            (
                NativeExpr::Bool(true),
                NativeExpr::Call {
                    function: 0,
                    args: vec![decrement(), NativeExpr::Param(2), NativeExpr::Param(1)],
                },
            ),
        ],
    });
    swap.arity = 3;
    swap.source_arity = 3;
    swap.params = vec![NativeType::Int, managed, managed];
    swap.return_type = managed;
    NativeModule {
        name: "app.Tail".to_string(),
        functions: vec![swap],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

fn managed_aggregate_countdown_module() -> NativeModule {
    let canonical = "TailBox";
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::constructor(
            canonical,
            "Box",
            0,
            1,
            vec![(Some("value".to_string()), ManagedFieldType::Int)],
        )
        .expect("tail aggregate descriptor"),
    );
    let encoded_layout = Arc::<[u8]>::from(
        encode_aggregate_layout(&descriptor).expect("encode tail aggregate descriptor"),
    );
    let managed = NativeType::ManagedRef(
        SemanticTypeId::from_canonical(canonical).expect("tail aggregate identity"),
    );
    let mut countdown = function(NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Equal,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(0)),
                    right: Box::new(NativeExpr::Int(0)),
                },
                NativeExpr::Param(1),
            ),
            (
                NativeExpr::Bool(true),
                NativeExpr::Call {
                    function: 0,
                    args: vec![decrement(), NativeExpr::Param(1)],
                },
            ),
        ],
    });
    countdown.export_id = 8;
    countdown.public = false;
    countdown.params = vec![NativeType::Int, managed];
    countdown.return_type = managed;
    let mut start = function(NativeExpr::Call {
        function: 0,
        args: vec![
            NativeExpr::Param(0),
            NativeExpr::Construct {
                descriptor,
                encoded_layout: encoded_layout.clone(),
                fields: vec![NativeExpr::Param(1)],
            },
        ],
    });
    start.name = "aggregate_start".to_string();
    start.source_function = start.name.clone();
    start.return_type = managed;
    NativeModule {
        name: "app.Tail".to_string(),
        functions: vec![countdown, start],
        continuations: Vec::new(),
        managed_layouts: vec![encoded_layout],
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }
}

fn managed_collection_countdown_module() -> NativeModule {
    let descriptor = ManagedCollectionDescriptor::list("List[Int]", ManagedFieldType::Int)
        .expect("tail collection descriptor");
    let semantic = descriptor.semantic_id();
    let encoded_layout = Arc::<[u8]>::from(
        encode_collection_layout(&descriptor).expect("encode tail collection descriptor"),
    );
    let managed = NativeType::ManagedRef(semantic);
    let mut countdown = function(NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Binary {
                    operator: NativeBinaryOperator::Equal,
                    operand_type: NativeType::Int,
                    left: Box::new(NativeExpr::Param(0)),
                    right: Box::new(NativeExpr::Int(0)),
                },
                NativeExpr::Param(1),
            ),
            (
                NativeExpr::Bool(true),
                NativeExpr::Call {
                    function: 0,
                    args: vec![decrement(), NativeExpr::Param(1)],
                },
            ),
        ],
    });
    countdown.export_id = 8;
    countdown.public = false;
    countdown.params = vec![NativeType::Int, managed];
    countdown.return_type = managed;
    let mut start = function(NativeExpr::Call {
        function: 0,
        args: vec![
            NativeExpr::Param(0),
            NativeExpr::ManagedOperation {
                encoded: Arc::from(encode_list_from_elements_operation(semantic)),
                args: vec![
                    NativeExpr::Param(1),
                    NativeExpr::Param(2),
                    NativeExpr::Param(3),
                ],
            },
        ],
    });
    start.name = "collection_start".to_string();
    start.source_function = start.name.clone();
    start.arity = 4;
    start.source_arity = 4;
    start.params = vec![
        NativeType::Int,
        NativeType::Int,
        NativeType::Int,
        NativeType::Int,
    ];
    start.return_type = managed;
    NativeModule {
        name: "app.Tail".to_string(),
        functions: vec![countdown, start],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: vec![encoded_layout],
        atoms: Vec::new(),
    }
}

fn split_mutual_countdown_modules() -> Vec<NativeModule> {
    let combined = mutual_countdown_module();
    let mut functions = combined.functions.into_iter();
    let mut even = functions.next().expect("even function");
    even.source_module = "app.TailEven".to_string();
    let mut odd = functions.next().expect("odd function");
    odd.source_module = "app.TailOdd".to_string();
    vec![
        NativeModule {
            name: "app.TailEven".to_string(),
            functions: vec![even],
            continuations: Vec::new(),
            managed_layouts: Vec::new(),
            managed_collections: Vec::new(),
            atoms: Vec::new(),
        },
        NativeModule {
            name: "app.TailOdd".to_string(),
            functions: vec![odd],
            continuations: Vec::new(),
            managed_layouts: Vec::new(),
            managed_collections: Vec::new(),
            atoms: Vec::new(),
        },
    ]
}

#[test]
fn direct_self_calls_are_terminal_only_in_result_forwarding_positions() {
    let nested_argument_call = NativeExpr::Call {
        function: 0,
        args: vec![decrement(), NativeExpr::Param(1)],
    };
    let body = NativeExpr::Let {
        bindings: vec![nested_argument_call.clone()],
        body: Box::new(NativeExpr::If {
            clauses: vec![
                (
                    NativeExpr::Bool(true),
                    NativeExpr::Call {
                        function: 0,
                        args: vec![decrement(), NativeExpr::Param(1)],
                    },
                ),
                (NativeExpr::Bool(true), NativeExpr::Param(1)),
            ],
        }),
    };
    let mut modules = vec![module(body)];

    lower_recursive_tail_calls(&mut modules);

    let NativeExpr::Let { bindings, body } = &modules[0].functions[0].body else {
        panic!("expected terminal let");
    };
    assert_eq!(bindings, &[nested_argument_call]);
    let NativeExpr::If { clauses } = body.as_ref() else {
        panic!("expected terminal if");
    };
    assert!(matches!(
        &clauses[0].1,
        NativeExpr::TailCall { function: 0, .. }
    ));
    assert_eq!(clauses[1].1, NativeExpr::Param(1));
}

#[test]
fn calls_followed_by_cleanup_are_not_tail_calls() {
    let call = NativeExpr::Call {
        function: 0,
        args: vec![decrement(), NativeExpr::Param(1)],
    };
    let mut modules = vec![module(NativeExpr::Try {
        protected: Box::new(NativeExpr::Int(0)),
        success: Box::new(call.clone()),
        failure: Box::new(NativeExpr::Int(-1)),
        cleanup: vec![NativeExpr::Unit],
    })];

    lower_recursive_tail_calls(&mut modules);

    let NativeExpr::Try { success, .. } = &modules[0].functions[0].body else {
        panic!("expected try expression");
    };
    assert_eq!(success.as_ref(), &call);
}

#[test]
fn recursive_component_rejects_terminal_dynamic_target_before_codegen() {
    let dynamic = NativeExpr::InvokeClosure {
        callee: Box::new(NativeExpr::Param(2)),
        args: vec![NativeExpr::Param(0)],
        parameter_types: vec![NativeType::Int],
        result_type: NativeType::Int,
    };
    let mut recursive = function(NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Param(0),
                NativeExpr::Call {
                    function: 0,
                    args: vec![decrement(), NativeExpr::Param(1)],
                },
            ),
            (NativeExpr::Bool(true), dynamic),
        ],
    });
    recursive.arity = 3;
    recursive.source_arity = 3;
    recursive.params = vec![NativeType::Int, NativeType::Int, NativeType::Int];
    let modules = vec![NativeModule {
        name: "app.Tail".to_string(),
        functions: vec![recursive],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }];

    assert_eq!(
        validate_recursive_tail_targets(&modules).unwrap_err(),
        "error[native_ir.dynamic_recursive_tail]: `app.Tail`.`loop`/3 has a terminal dynamically selected target that cannot satisfy the compiler-owned constant-stack contract"
    );
}

#[test]
fn mutual_component_accepts_managed_scalar_slot_mismatch_for_typed_lanes() {
    let mut module = mutual_countdown_module();
    module.functions[1].params[1] = NativeType::ManagedRef(
        SemanticTypeId::from_canonical("TailToken").expect("managed tail token identity"),
    );
    module.functions[1].return_type = module.functions[1].params[1];

    validate_recursive_tail_targets(&[module])
        .expect("managed and scalar arguments occupy distinct dispatcher lanes");
}

#[test]
fn mutually_recursive_component_rewrites_only_its_terminal_internal_edges() {
    let mut first = function(NativeExpr::Call {
        function: 1,
        args: vec![NativeExpr::Param(0), NativeExpr::Param(1)],
    });
    first.name = "first".to_string();
    first.source_function = first.name.clone();
    let mut second = function(NativeExpr::If {
        clauses: vec![
            (
                NativeExpr::Param(0),
                NativeExpr::Call {
                    function: 0,
                    args: vec![NativeExpr::Param(0), NativeExpr::Param(1)],
                },
            ),
            (
                NativeExpr::Bool(true),
                NativeExpr::Call {
                    function: 2,
                    args: vec![NativeExpr::Param(0), NativeExpr::Param(1)],
                },
            ),
        ],
    });
    second.name = "second".to_string();
    second.source_function = second.name.clone();
    let mut terminal = function(NativeExpr::Param(1));
    terminal.name = "terminal".to_string();
    terminal.source_function = terminal.name.clone();
    let mut modules = vec![NativeModule {
        name: "app.Tail".to_string(),
        functions: vec![first, second, terminal],
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }];

    assert_eq!(mutual_tail_components(&modules), vec![vec![0, 1]]);
    lower_recursive_tail_calls(&mut modules);

    assert!(matches!(
        modules[0].functions[0].body,
        NativeExpr::TailCall { function: 1, .. }
    ));
    let NativeExpr::If { clauses } = &modules[0].functions[1].body else {
        panic!("expected mutually recursive branch");
    };
    assert!(matches!(
        clauses[0].1,
        NativeExpr::TailCall { function: 0, .. }
    ));
    assert!(matches!(clauses[1].1, NativeExpr::Call { function: 2, .. }));
}

#[test]
fn materialized_identity_completion_uses_typed_dispatcher_lanes() {
    let call_then =
        |function, completion_continuation_id, completion_function| NativeExpr::CallThen {
            function,
            args: vec![NativeExpr::Param(0), NativeExpr::Param(1)],
            resumes: Vec::new(),
            completion_continuation_id,
            completion_function: Some(completion_function),
            values: Vec::new(),
        };
    let mut entry = function(call_then(1, 10, 2));
    entry.name = "entry".to_string();
    let mut leaf = function(NativeExpr::Int(1));
    leaf.name = "leaf".to_string();
    let mut completion = function(call_then(0, 11, 3));
    completion.name = "$continuation_step".to_string();
    completion.params[1] = NativeType::ManagedRef(
        SemanticTypeId::from_canonical("CompletionState").expect("completion state identity"),
    );
    let mut identity = function(NativeExpr::Param(0));
    identity.name = "$continuation_identity".to_string();
    identity.arity = 1;
    identity.source_arity = 1;
    identity.params.truncate(1);
    let continuation = |id, function| NativeContinuation {
        id,
        source_module: "app.ControlCycle".to_string(),
        source_function: "entry".to_string(),
        source_arity: 2,
        source_span: None,
        capture_names: vec!["value".to_string()],
        params: vec![NativeType::Int],
        return_type: NativeType::Int,
        body: NativeExpr::TailCall {
            function,
            args: vec![NativeExpr::Param(0)],
            yield_continuation_id: None,
        },
    };
    let mut modules = vec![NativeModule {
        name: "app.ControlCycle".to_string(),
        functions: vec![entry, leaf, completion, identity],
        continuations: vec![continuation(10, 2), continuation(11, 3)],
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }];

    lower_recursive_tail_calls(&mut modules);

    assert!(matches!(
        modules[0].functions[2].body,
        NativeExpr::TailCall { function: 0, .. }
    ));
    assert_eq!(mutual_tail_components(&modules), vec![vec![0, 2]]);
    validate_recursive_tail_targets(&modules)
        .expect("generated completions use separately typed dispatcher lanes");
}

#[test]
fn iterative_scc_analysis_lowers_a_ten_thousand_function_recursive_component() {
    const FUNCTION_COUNT: usize = 10_000;
    let functions = (0..FUNCTION_COUNT)
        .map(|index| {
            let target = (index + 1) % FUNCTION_COUNT;
            let mut member = function(NativeExpr::Call {
                function: target,
                args: vec![NativeExpr::Param(0), NativeExpr::Param(1)],
            });
            member.export_id = index as u64 + 1;
            member.name = format!("member_{index}");
            member.source_function = member.name.clone();
            member
        })
        .collect();
    let mut modules = vec![NativeModule {
        name: "app.LargeTailComponent".to_string(),
        functions,
        continuations: Vec::new(),
        managed_layouts: Vec::new(),
        managed_collections: Vec::new(),
        atoms: Vec::new(),
    }];

    lower_recursive_tail_calls(&mut modules);

    assert!(modules[0]
        .functions
        .iter()
        .all(|function| matches!(function.body, NativeExpr::TailCall { .. })));
}

#[cfg(unix)]
#[test]
fn object_backedge_invariant_is_sensitive_to_the_tail_position_transform() {
    let mut modules = vec![module(deep_countdown_body())];
    let unlowered = super::emit_native_application_object("unlowered_tail_control", &modules)
        .expect("emit unlowered control object");
    assert_component_relocation(&unlowered, "loop", 2);

    lower_recursive_tail_calls(&mut modules);
    let lowered = super::emit_native_application_object("lowered_tail_control", &modules)
        .expect("emit lowered control object");
    assert_no_self_relocation(&lowered);
}

#[cfg(unix)]
#[test]
fn direct_aot_executes_one_million_self_tail_calls_on_a_small_native_stack() {
    let mut modules = vec![module(deep_countdown_body())];
    lower_recursive_tail_calls(&mut modules);
    let object = super::emit_native_application_object("tail_loop", &modules)
        .expect("emit direct tail-loop object");
    assert_no_self_relocation(&object);

    let root = std::env::temp_dir().join(format!(
        "terlan-tail-loop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create tail-loop fixture");
    let object_path = root.join("tail-loop.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write tail-loop object");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(DEEP_TAIL_LOOP_HARNESS),
    )
    .expect("write tail-loop harness");
    compile_and_run_with_small_stack(&object_path, &harness_path, &executable_path);
    fs::remove_dir_all(root).expect("remove tail-loop fixture");
}

#[cfg(unix)]
#[test]
fn direct_aot_preserves_terminal_let_bindings_across_one_million_backedges() {
    let mut modules = vec![module(deep_let_countdown_body())];
    lower_recursive_tail_calls(&mut modules);
    let object = super::emit_native_application_object("tail_let_loop", &modules)
        .expect("emit direct terminal-let tail-loop object");
    assert_no_self_relocation(&object);

    let root = std::env::temp_dir().join(format!(
        "terlan-tail-let-loop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create terminal-let tail-loop fixture");
    let object_path = root.join("tail-loop.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write terminal-let tail-loop object");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(DEEP_TAIL_LOOP_HARNESS),
    )
    .expect("write terminal-let tail-loop harness");
    compile_and_run_with_small_stack(&object_path, &harness_path, &executable_path);
    fs::remove_dir_all(root).expect("remove terminal-let tail-loop fixture");
}

#[cfg(unix)]
#[test]
fn suspending_self_tail_calls_reuse_the_entry_frame_before_forwarding_transition() {
    let mut modules = vec![module(suspending_countdown_body())];
    lower_recursive_tail_calls(&mut modules);
    let object = super::emit_native_application_object("suspending_tail_loop", &modules)
        .expect("emit suspending tail-loop object");
    assert_no_self_relocation(&object);

    let root = std::env::temp_dir().join(format!(
        "terlan-suspending-tail-loop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create suspending tail-loop fixture");
    let object_path = root.join("tail-loop.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write suspending tail-loop object");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(SUSPENDING_TAIL_LOOP_HARNESS),
    )
    .expect("write suspending tail-loop harness");
    compile_and_run_with_small_stack(&object_path, &harness_path, &executable_path);
    fs::remove_dir_all(root).expect("remove suspending tail-loop fixture");
}

#[cfg(unix)]
#[test]
fn direct_aot_executes_one_million_mutual_tail_calls_through_bounded_dispatch() {
    let mut modules = vec![mutual_countdown_module()];
    lower_recursive_tail_calls(&mut modules);
    let object = super::emit_native_application_object("mutual_tail_loop", &modules)
        .expect("emit mutual tail-loop object");
    assert_no_component_relocation(&object, &[("even", 2), ("odd", 2)]);

    let root = std::env::temp_dir().join(format!(
        "terlan-mutual-tail-loop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create mutual tail-loop fixture");
    let object_path = root.join("tail-loop.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write mutual tail-loop object");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(DEEP_TAIL_LOOP_HARNESS),
    )
    .expect("write mutual tail-loop harness");
    compile_and_run_with_small_stack(&object_path, &harness_path, &executable_path);
    fs::remove_dir_all(root).expect("remove mutual tail-loop fixture");
}

#[cfg(unix)]
#[test]
fn mutual_tail_dispatch_pads_heterogeneous_arities_without_exposing_padding() {
    let mut modules = vec![heterogeneous_mutual_countdown_module()];
    lower_recursive_tail_calls(&mut modules);
    let object = super::emit_native_application_object("heterogeneous_tail_loop", &modules)
        .expect("emit heterogeneous mutual tail-loop object");
    assert_no_component_relocation(&object, &[("narrow", 1), ("wide", 2)]);

    let root = std::env::temp_dir().join(format!(
        "terlan-heterogeneous-tail-loop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create heterogeneous tail-loop fixture");
    let object_path = root.join("tail-loop.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write heterogeneous tail-loop object");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(HETEROGENEOUS_TAIL_LOOP_HARNESS),
    )
    .expect("write heterogeneous tail-loop harness");
    compile_and_run_with_small_stack(&object_path, &harness_path, &executable_path);
    fs::remove_dir_all(root).expect("remove heterogeneous tail-loop fixture");
}

#[cfg(unix)]
#[test]
fn suspending_mutual_tail_calls_reuse_one_frame_and_preserve_transition_abi() {
    let mut modules = vec![suspending_mutual_countdown_module()];
    lower_recursive_tail_calls(&mut modules);
    let object = super::emit_native_application_object("suspending_mutual_tail_loop", &modules)
        .expect("emit suspending mutual tail-loop object");
    assert_no_component_relocation(&object, &[("even", 2), ("odd", 2)]);

    let root = std::env::temp_dir().join(format!(
        "terlan-suspending-mutual-tail-loop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create suspending mutual tail-loop fixture");
    let object_path = root.join("tail-loop.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write suspending mutual tail-loop object");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(SUSPENDING_TAIL_LOOP_HARNESS),
    )
    .expect("write suspending mutual tail-loop harness");
    compile_and_run_with_small_stack(&object_path, &harness_path, &executable_path);
    fs::remove_dir_all(root).expect("remove suspending mutual tail-loop fixture");
}

#[cfg(unix)]
#[test]
fn cancellation_after_one_million_mutual_edges_preserves_resume_identity_and_capture() {
    let mut modules = vec![cancelling_mutual_countdown_module()];
    lower_recursive_tail_calls(&mut modules);
    let object = super::emit_native_application_object("cancelling_mutual_tail_loop", &modules)
        .expect("emit cancelling mutual tail-loop object");
    assert_no_component_relocation(&object, &[("cancel_even", 2), ("cancel_odd", 2)]);

    let root = std::env::temp_dir().join(format!(
        "terlan-cancelling-mutual-tail-loop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create cancelling mutual tail-loop fixture");
    let object_path = root.join("tail-loop.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write cancelling mutual tail-loop object");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(CANCELLING_TAIL_LOOP_HARNESS),
    )
    .expect("write cancelling mutual tail-loop harness");
    compile_and_run_with_small_stack(&object_path, &harness_path, &executable_path);
    fs::remove_dir_all(root).expect("remove cancelling mutual tail-loop fixture");
}

#[cfg(unix)]
#[test]
fn managed_reference_identity_survives_one_million_mutual_tail_edges() {
    let mut modules = vec![managed_mutual_countdown_module()];
    lower_recursive_tail_calls(&mut modules);
    let object = super::emit_native_application_object("managed_mutual_tail_loop", &modules)
        .expect("emit managed mutual tail-loop object");
    assert_no_component_relocation(&object, &[("managed_even", 2), ("managed_odd", 2)]);

    let root = std::env::temp_dir().join(format!(
        "terlan-managed-mutual-tail-loop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create managed mutual tail-loop fixture");
    let object_path = root.join("tail-loop.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write managed mutual tail-loop object");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(MANAGED_TAIL_LOOP_HARNESS),
    )
    .expect("write managed mutual tail-loop harness");
    compile_and_run_with_small_stack(&object_path, &harness_path, &executable_path);
    fs::remove_dir_all(root).expect("remove managed mutual tail-loop fixture");
}

#[cfg(unix)]
#[test]
fn managed_tail_arguments_are_replaced_in_parallel_before_the_backedge() {
    let mut modules = vec![managed_parallel_swap_module()];
    lower_recursive_tail_calls(&mut modules);
    let object = super::emit_native_application_object("managed_parallel_tail_loop", &modules)
        .expect("emit managed parallel tail-loop object");
    assert_no_component_relocation(&object, &[("loop", 3)]);

    let root = std::env::temp_dir().join(format!(
        "terlan-managed-parallel-tail-loop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create managed parallel tail-loop fixture");
    let object_path = root.join("tail-loop.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write managed parallel tail-loop object");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(MANAGED_PARALLEL_TAIL_LOOP_HARNESS),
    )
    .expect("write managed parallel tail-loop harness");
    compile_and_run_with_small_stack(&object_path, &harness_path, &executable_path);
    fs::remove_dir_all(root).expect("remove managed parallel tail-loop fixture");
}

#[cfg(unix)]
#[test]
fn managed_aggregate_is_allocated_once_and_survives_one_million_tail_edges() {
    let mut modules = vec![managed_aggregate_countdown_module()];
    lower_recursive_tail_calls(&mut modules);
    let object = super::emit_native_application_object("managed_aggregate_tail_loop", &modules)
        .expect("emit managed aggregate tail-loop object");
    assert_no_self_relocation(&object);

    let root = std::env::temp_dir().join(format!(
        "terlan-managed-aggregate-tail-loop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create managed aggregate tail-loop fixture");
    let object_path = root.join("tail-loop.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write managed aggregate tail-loop object");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(MANAGED_AGGREGATE_TAIL_LOOP_HARNESS),
    )
    .expect("write managed aggregate tail-loop harness");
    compile_and_run_with_small_stack(&object_path, &harness_path, &executable_path);
    fs::remove_dir_all(root).expect("remove managed aggregate tail-loop fixture");
}

#[cfg(unix)]
#[test]
fn managed_collection_is_allocated_once_and_survives_one_million_tail_edges() {
    let mut modules = vec![managed_collection_countdown_module()];
    lower_recursive_tail_calls(&mut modules);
    let object = super::emit_native_application_object("managed_collection_tail_loop", &modules)
        .expect("emit managed collection tail-loop object");
    assert_no_self_relocation(&object);

    let root = std::env::temp_dir().join(format!(
        "terlan-managed-collection-tail-loop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create managed collection tail-loop fixture");
    let object_path = root.join("tail-loop.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write managed collection tail-loop object");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(MANAGED_COLLECTION_TAIL_LOOP_HARNESS),
    )
    .expect("write managed collection tail-loop harness");
    compile_and_run_with_small_stack(&object_path, &harness_path, &executable_path);
    fs::remove_dir_all(root).expect("remove managed collection tail-loop fixture");
}

#[cfg(unix)]
#[test]
fn checked_failure_after_one_million_tail_edges_uses_the_canonical_status_path() {
    let mut modules = vec![module(failing_countdown_body())];
    lower_recursive_tail_calls(&mut modules);
    let object = super::emit_native_application_object("failing_tail_loop", &modules)
        .expect("emit checked-failure tail-loop object");
    assert_no_self_relocation(&object);

    let root = std::env::temp_dir().join(format!(
        "terlan-failing-tail-loop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create checked-failure tail-loop fixture");
    let object_path = root.join("tail-loop.o");
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(&object_path, object).expect("write checked-failure tail-loop object");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(FAILING_TAIL_LOOP_HARNESS),
    )
    .expect("write checked-failure tail-loop harness");
    compile_and_run_with_small_stack(&object_path, &harness_path, &executable_path);
    fs::remove_dir_all(root).expect("remove checked-failure tail-loop fixture");
}

#[cfg(unix)]
#[test]
fn non_tail_recursion_remains_a_real_call_and_is_not_flattened() {
    let mut modules = vec![module(non_tail_countdown_body())];
    lower_recursive_tail_calls(&mut modules);
    let NativeExpr::If { clauses } = &modules[0].functions[0].body else {
        panic!("expected recursive conditional");
    };
    let NativeExpr::Binary { left, .. } = &clauses[1].1 else {
        panic!("expected post-call arithmetic");
    };
    assert!(matches!(
        left.as_ref(),
        NativeExpr::Call { function: 0, .. }
    ));

    let object = super::emit_native_application_object("non_tail_recursion", &modules)
        .expect("emit non-tail recursive object");
    assert_component_relocation(&object, "loop", 2);
}

#[cfg(unix)]
#[test]
fn split_module_units_embed_mutual_dispatch_without_cross_unit_recursive_calls() {
    let mut modules = split_mutual_countdown_modules();
    lower_recursive_tail_calls(&mut modules);
    let policy = super::NativeCodegenPolicy::Development;
    let even = super::emit_native_module_object_with_policy("split_tail", &modules, 0, policy)
        .expect("emit even module unit");
    let odd = super::emit_native_module_object_with_policy("split_tail", &modules, 1, policy)
        .expect("emit odd module unit");
    let dispatch =
        super::emit_native_application_dispatch_object_with_policy("split_tail", &modules, policy)
            .expect("emit split application dispatch");
    let component = [("app.TailEven", "even", 2), ("app.TailOdd", "odd", 2)];
    assert_defined_component_has_no_recursive_relocation(
        &even,
        "app.TailEven",
        "even",
        2,
        &component,
    );
    assert_defined_component_has_no_recursive_relocation(&odd, "app.TailOdd", "odd", 2, &component);

    let root = std::env::temp_dir().join(format!(
        "terlan-split-mutual-tail-loop-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("create split mutual tail-loop fixture");
    let objects =
        [("even.o", even), ("odd.o", odd), ("dispatch.o", dispatch)].map(|(name, object)| {
            let path = root.join(name);
            fs::write(&path, object).expect("write split mutual tail-loop object");
            path
        });
    let harness_path = root.join("harness.rs");
    let executable_path = root.join("harness");
    fs::write(
        &harness_path,
        with_dispatch_lookup_harness(DEEP_TAIL_LOOP_HARNESS),
    )
    .expect("write split mutual tail-loop harness");
    compile_and_run_many_with_small_stack(&objects, &harness_path, &executable_path);
    fs::remove_dir_all(root).expect("remove split mutual tail-loop fixture");
}

#[cfg(unix)]
const DEEP_TAIL_LOOP_HARNESS: &str = r#"
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
    let arguments = [1_000_000_i64, 0_i64];
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, 1_000_000);
    assert_eq!(transition_len, 0);
}
"#;

#[cfg(unix)]
const SUSPENDING_TAIL_LOOP_HARNESS: &str = r#"
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
    let arguments = [1_000_000_i64, 0_i64];
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 6);
    assert_eq!(result, 91);
    assert_eq!(transition_len, 1);
    assert_eq!(transitions[0], 1_000_000);
}
"#;

#[cfg(unix)]
const HETEROGENEOUS_TAIL_LOOP_HARNESS: &str = r#"
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
    let arguments = [1_000_000_i64];
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
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

#[cfg(unix)]
const MANAGED_TAIL_LOOP_HARNESS: &str = r#"
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
    const MANAGED_TOKEN: i64 = 0x5a5a_1234;
    let arguments = [1_000_000_i64, MANAGED_TOKEN];
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, MANAGED_TOKEN);
    assert_eq!(transition_len, 0);
}
"#;

#[cfg(unix)]
const MANAGED_PARALLEL_TAIL_LOOP_HARNESS: &str = r#"
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
    const LEFT: i64 = 0x1111_2222;
    const RIGHT: i64 = 0x3333_4444;
    let arguments = [1_000_001_i64, LEFT, RIGHT];
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, LEFT);
    assert_eq!(transition_len, 0);
}
"#;

#[cfg(unix)]
const FAILING_TAIL_LOOP_HARNESS: &str = r#"
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
    let arguments = [1_000_000_i64, 0_i64];
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 4);
    assert_eq!(transition_len, 0);
}
"#;

#[cfg(unix)]
const MANAGED_AGGREGATE_TAIL_LOOP_HARNESS: &str = r#"
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

#[derive(Default)]
struct Capture {
    calls: usize,
    fields: Vec<i64>,
}

unsafe extern "C" fn allocate(
    context: *mut c_void,
    layout: *const u8,
    layout_len: u64,
    fields: *const i64,
    field_count: u64,
    result: *mut u64,
) -> i32 {
    let capture = unsafe { &mut *context.cast::<Capture>() };
    let layout = unsafe { std::slice::from_raw_parts(layout, layout_len as usize) };
    assert_eq!(&layout[..4], b"TVMA");
    capture.calls += 1;
    capture.fields =
        unsafe { std::slice::from_raw_parts(fields, field_count as usize).to_vec() };
    unsafe { *result = 0x5a5a_1234 };
    0
}

fn main() {
    let arguments = [1_000_000_i64, 42_i64];
    let mut capture = Capture::default();
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            (&mut capture as *mut Capture).cast(),
            allocate as *const () as *const c_void,
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, 0x5a5a_1234);
    assert_eq!(transition_len, 0);
    assert_eq!(capture.calls, 1);
    assert_eq!(capture.fields, [42]);
}
"#;

#[cfg(unix)]
const MANAGED_COLLECTION_TAIL_LOOP_HARNESS: &str = r#"
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

#[derive(Default)]
struct Capture {
    calls: usize,
    fields: Vec<i64>,
}

unsafe extern "C" fn allocate(
    context: *mut c_void,
    operation: *const u8,
    operation_len: u64,
    fields: *const i64,
    field_count: u64,
    result: *mut u64,
) -> i32 {
    let capture = unsafe { &mut *context.cast::<Capture>() };
    let operation = unsafe { std::slice::from_raw_parts(operation, operation_len as usize) };
    assert_eq!(&operation[..4], b"TVMC");
    capture.calls += 1;
    capture.fields =
        unsafe { std::slice::from_raw_parts(fields, field_count as usize).to_vec() };
    unsafe { *result = 0x6b6b_2345 };
    0
}

fn main() {
    let arguments = [1_000_000_i64, 1_i64, 2_i64, 3_i64];
    let mut capture = Capture::default();
    let mut result = -1_i64;
    let mut transitions = [0_i64; 1];
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            (&mut capture as *mut Capture).cast(),
            allocate as *const () as *const c_void,
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            7,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    assert_eq!(status, 0);
    assert_eq!(result, 0x6b6b_2345);
    assert_eq!(transition_len, 0);
    assert_eq!(capture.calls, 1);
    assert_eq!(capture.fields, [1, 2, 3]);
}
"#;

#[cfg(unix)]
const CANCELLING_TAIL_LOOP_HARNESS: &str = r#"
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

fn dispatch(export_id: u64, arguments: &[i64], transitions: &mut [i64]) -> (i32, i64, u64) {
    let mut result = -1_i64;
    let mut transition_len = 99_u64;
    let status = unsafe {
        terlan_native_dispatch_v3(
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            dispatch_lookup as *const c_void,
            export_id,
            arguments.as_ptr(),
            arguments.len() as u64,
            &mut result,
            transitions.as_mut_ptr(),
            transitions.len() as u64,
            &mut transition_len,
        )
    };
    (status, result, transition_len)
}

fn main() {
    let mut transitions = [0_i64; 2];
    let (status, continuation, transition_len) =
        dispatch(7, &[1_000_000, 0], &mut transitions);
    assert_eq!(status, 15);
    assert_eq!(continuation, 93);
    assert_eq!(transition_len, 2);
    assert_eq!(transitions, [123, 1_000_000]);

    let captured = transitions[1];
    let (status, result, transition_len) = dispatch(93, &[captured], &mut transitions);
    assert_eq!(status, 0);
    assert_eq!(result, 1_000_001);
    assert_eq!(transition_len, 0);
}
"#;
