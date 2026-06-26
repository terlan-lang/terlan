use super::ContractBaseline;

/// Returns the gate-backed CoreIR contract baseline table.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static slice of LP8 compiler fixtures and their required CoreIR contract
///   snippets.
///
/// Transformation:
/// - Exposes immutable proof-baseline expectations for callers that already
///   know how to produce actual CoreIR contract text.
pub(crate) const fn contract_baselines() -> &'static [ContractBaseline] {
    &[
        ContractBaseline {
            module_name: "phase_basic",
            required_snippets: &[
                "body=BinaryOp:core=BinaryOp(+;Var(X), Var(Y)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=BinaryOp(+;Var(X), Var(Y))):proof=lean-covered",
                "children=[Var:core=Var(X):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(X)):proof=lean-covered:text=X:arity=0;Var:core=Var(Y):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(Y)):proof=lean-covered:text=Y:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:3 summary_only_expr:0 typed_core_pattern:2 summary_only_pattern:0 typed_core_type:3 summary_only_type:0",
                "checked_preservation_expr:3 checked_preservation_pattern:2 checked_preservation_expr_structural:3 checked_preservation_pattern_structural:2",
            ],
        },
        ContractBaseline {
            module_name: "phase_binary_eq",
            required_snippets: &[
                "body=BinaryOp:core=BinaryOp(==;Var(x), Var(y)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=BinaryOp(==;Var(x), Var(y))):proof=lean-covered",
                "children=[Var:core=Var(x):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(x)):proof=lean-covered:text=x:arity=0;Var:core=Var(y):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(y)):proof=lean-covered:text=y:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:3 summary_only_expr:0 typed_core_pattern:2 summary_only_pattern:0 typed_core_type:3 summary_only_type:0",
                "checked_preservation_expr:3 checked_preservation_pattern:2 checked_preservation_expr_structural:3 checked_preservation_pattern_structural:2",
            ],
        },
        ContractBaseline {
            module_name: "phase_binary_lt",
            required_snippets: &[
                "body=BinaryOp:core=BinaryOp(<;Var(x), Var(y)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=BinaryOp(<;Var(x), Var(y))):proof=lean-covered",
                "children=[Var:core=Var(x):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(x)):proof=lean-covered:text=x:arity=0;Var:core=Var(y):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(y)):proof=lean-covered:text=y:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:3 summary_only_expr:0 typed_core_pattern:2 summary_only_pattern:0 typed_core_type:3 summary_only_type:0",
                "checked_preservation_expr:3 checked_preservation_pattern:2 checked_preservation_expr_structural:3 checked_preservation_pattern_structural:2",
            ],
        },
        ContractBaseline {
            module_name: "phase_binary_lte",
            required_snippets: &[
                "body=BinaryOp:core=BinaryOp(<=;Var(x), Var(y)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=BinaryOp(<=;Var(x), Var(y))):proof=lean-covered",
                "children=[Var:core=Var(x):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(x)):proof=lean-covered:text=x:arity=0;Var:core=Var(y):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(y)):proof=lean-covered:text=y:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:3 summary_only_expr:0 typed_core_pattern:2 summary_only_pattern:0 typed_core_type:3 summary_only_type:0",
                "checked_preservation_expr:3 checked_preservation_pattern:2 checked_preservation_expr_structural:3 checked_preservation_pattern_structural:2",
            ],
        },
        ContractBaseline {
            module_name: "phase_binary_gt",
            required_snippets: &[
                "body=BinaryOp:core=BinaryOp(>;Var(x), Var(y)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=BinaryOp(>;Var(x), Var(y))):proof=lean-covered",
                "children=[Var:core=Var(x):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(x)):proof=lean-covered:text=x:arity=0;Var:core=Var(y):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(y)):proof=lean-covered:text=y:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:3 summary_only_expr:0 typed_core_pattern:2 summary_only_pattern:0 typed_core_type:3 summary_only_type:0",
                "checked_preservation_expr:3 checked_preservation_pattern:2 checked_preservation_expr_structural:3 checked_preservation_pattern_structural:2",
            ],
        },
        ContractBaseline {
            module_name: "phase_binary_gte",
            required_snippets: &[
                "body=BinaryOp:core=BinaryOp(>=;Var(x), Var(y)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=BinaryOp(>=;Var(x), Var(y))):proof=lean-covered",
                "children=[Var:core=Var(x):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(x)):proof=lean-covered:text=x:arity=0;Var:core=Var(y):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(y)):proof=lean-covered:text=y:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:3 summary_only_expr:0 typed_core_pattern:2 summary_only_pattern:0 typed_core_type:3 summary_only_type:0",
                "checked_preservation_expr:3 checked_preservation_pattern:2 checked_preservation_expr_structural:3 checked_preservation_pattern_structural:2",
            ],
        },
        ContractBaseline {
            module_name: "phase_binary_mul",
            required_snippets: &[
                "body=BinaryOp:core=BinaryOp(*;Var(x), Var(y)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=BinaryOp(*;Var(x), Var(y))):proof=lean-covered",
                "children=[Var:core=Var(x):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(x)):proof=lean-covered:text=x:arity=0;Var:core=Var(y):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(y)):proof=lean-covered:text=y:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:3 summary_only_expr:0 typed_core_pattern:2 summary_only_pattern:0 typed_core_type:3 summary_only_type:0",
                "checked_preservation_expr:3 checked_preservation_pattern:2 checked_preservation_expr_structural:3 checked_preservation_pattern_structural:2",
            ],
        },
        ContractBaseline {
            module_name: "phase_binary_sub",
            required_snippets: &[
                "body=BinaryOp:core=BinaryOp(-;Var(x), Var(y)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=BinaryOp(-;Var(x), Var(y))):proof=lean-covered",
                "children=[Var:core=Var(x):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(x)):proof=lean-covered:text=x:arity=0;Var:core=Var(y):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(y)):proof=lean-covered:text=y:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:3 summary_only_expr:0 typed_core_pattern:2 summary_only_pattern:0 typed_core_type:3 summary_only_type:0",
                "checked_preservation_expr:3 checked_preservation_pattern:2 checked_preservation_expr_structural:3 checked_preservation_pattern_structural:2",
            ],
        },
        ContractBaseline {
            module_name: "phase_core_lean",
            required_snippets: &[
                "body=Var:core=Var(X):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(X)):proof=lean-covered",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:1 summary_only_expr:0 typed_core_pattern:1 summary_only_pattern:0 typed_core_type:2 summary_only_type:0",
            ],
        },
        ContractBaseline {
            module_name: "phase_int_literal",
            required_snippets: &[
                "body=Int:core=Int(42):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Int(42)):proof=lean-covered",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:1 summary_only_expr:0 typed_core_pattern:0 summary_only_pattern:0 typed_core_type:1 summary_only_type:0",
                "checked_preservation_expr:1 checked_preservation_pattern:0 checked_preservation_expr_structural:1 checked_preservation_pattern_structural:0",
            ],
        },
        ContractBaseline {
            module_name: "phase_atom_literal",
            required_snippets: &[
                "body=Atom:core=Atom(ok):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Atom(ok)):proof=lean-covered",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:1 summary_only_expr:0 typed_core_pattern:0 summary_only_pattern:0 typed_core_type:1 summary_only_type:0",
                "checked_preservation_expr:1 checked_preservation_pattern:0 checked_preservation_expr_structural:1 checked_preservation_pattern_structural:0",
            ],
        },
        ContractBaseline {
            module_name: "phase_binary_literal",
            required_snippets: &[
                "body=Binary:core=Binary(\"hello\"):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Binary(\"hello\")):proof=lean-covered",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:1 summary_only_expr:0 typed_core_pattern:0 summary_only_pattern:0 typed_core_type:1 summary_only_type:0",
                "checked_preservation_expr:1 checked_preservation_pattern:0 checked_preservation_expr_structural:1 checked_preservation_pattern_structural:0",
            ],
        },
        ContractBaseline {
            module_name: "phase_tuple_literal",
            required_snippets: &[
                "return={Int , Int } return_core=Tuple(Int,Int)",
                "body=Tuple:core=Tuple(Int(1),Int(2)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Tuple(Int(1),Int(2))):proof=lean-covered",
                "children=[Int:core=Int(1):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Int(1)):proof=lean-covered:text=1:arity=0;Int:core=Int(2):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Int(2)):proof=lean-covered:text=2:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:3 summary_only_expr:0 typed_core_pattern:0 summary_only_pattern:0 typed_core_type:1 summary_only_type:0",
                "checked_preservation_expr:3 checked_preservation_pattern:0 checked_preservation_expr_structural:3 checked_preservation_pattern_structural:0",
            ],
        },
        ContractBaseline {
            module_name: "phase_list_literal",
            required_snippets: &[
                "return=List[Int] return_core=List(Int)",
                "body=List:core=List(Int(1),Int(2)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=List(Int(1),Int(2))):proof=lean-covered",
                "children=[Int:core=Int(1):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Int(1)):proof=lean-covered:text=1:arity=0;Int:core=Int(2):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Int(2)):proof=lean-covered:text=2:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:3 summary_only_expr:0 typed_core_pattern:0 summary_only_pattern:0 typed_core_type:1 summary_only_type:0",
                "checked_preservation_expr:3 checked_preservation_pattern:0 checked_preservation_expr_structural:3 checked_preservation_pattern_structural:0",
            ],
        },
        ContractBaseline {
            module_name: "phase_named_call",
            required_snippets: &[
                "function=identity/1 public=false params=x:Int:core=Int return=Int return_core=Int",
                "body=Call:core=Call(identity;Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Call(identity;Int(1))):proof=lean-covered",
                "children=[Var:core=Var(identity):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(identity)):proof=lean-covered:text=identity:arity=0;Int:core=Int(1):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Int(1)):proof=lean-covered:text=1:arity=0]",
                "metadata=functions:2 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:4 summary_only_expr:0 typed_core_pattern:1 summary_only_pattern:0 typed_core_type:3 summary_only_type:0",
                "checked_preservation_expr:4 checked_preservation_pattern:1 checked_preservation_expr_structural:4 checked_preservation_pattern_structural:1",
            ],
        },
        ContractBaseline {
            module_name: "phase_unary_operator",
            required_snippets: &[
                "body=UnaryOp:core=UnaryOp(-;Var(value)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=UnaryOp(-;Var(value))):proof=lean-covered",
                "children=[Var:core=Var(value):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(value)):proof=lean-covered:text=value:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:2 summary_only_expr:0 typed_core_pattern:1 summary_only_pattern:0 typed_core_type:2 summary_only_type:0",
                "checked_preservation_expr:2 checked_preservation_pattern:1 checked_preservation_expr_structural:2 checked_preservation_pattern_structural:1",
            ],
        },
        ContractBaseline {
            module_name: "phase_core_lambda",
            required_snippets: &[
                "body=Fun:core=Lam(Var(x);Var(x)):preservation=structural-core-expr(freshness=runtime-bindings-required;target=Lam(Var(x);Var(x))):proof=lean-covered",
                "Var:core=Var(x):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(x)):proof=lean-covered",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "checked_preservation_expr_no_runtime_bindings:1 checked_preservation_pattern_no_runtime_bindings:0 checked_preservation_expr_runtime_bindings_required:1",
            ],
        },
        ContractBaseline {
            module_name: "phase_constructor_resolution",
            required_snippets: &[
                "body=Call:core=ConstructorCall(Ok;identity=Ok;Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=ConstructorCall(Ok;identity=Ok;Int(1))):proof=lean-covered",
                "metadata=functions:1 types:0 constructors:1 proof_readiness:lean-covered",
                "resolved_constructor_call_identity:1 resolved_constructor_chain_identity:0 resolved_constructor_pattern_identity:0",
            ],
        },
        ContractBaseline {
            module_name: "phase_constructor_pattern_resolution",
            required_snippets: &[
                "body=Case:core=Case(Var(input);Constructor(Some;identity=Some;Var(value))=>Var(value)):preservation=structural-core-expr(freshness=runtime-bindings-required;target=Case(Var(input);Constructor(Some;identity=Some;Var(value))=>Var(value))):proof=lean-covered",
                "metadata=functions:1 types:0 constructors:1 proof_readiness:lean-covered",
                "checked_preservation_expr_no_runtime_bindings:2 checked_preservation_pattern_no_runtime_bindings:0 checked_preservation_expr_runtime_bindings_required:1 checked_preservation_pattern_runtime_bindings_required:1",
                "resolved_constructor_call_identity:0 resolved_constructor_chain_identity:0 resolved_constructor_pattern_identity:1",
            ],
        },
        ContractBaseline {
            module_name: "phase_literal_pattern_case",
            required_snippets: &[
                "body=Case:core=Case(Var(status);Atom(none)=>Int(0)|Wildcard=>Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Case(Var(status);Atom(none)=>Int(0)|Wildcard=>Int(1))):proof=lean-covered",
                "children=[Var:core=Var(status):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(status)):proof=lean-covered:text=status:arity=0;Int:core=Int(0):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Int(0)):proof=lean-covered:text=0:arity=0;Int:core=Int(1):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Int(1)):proof=lean-covered:text=1:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:4 summary_only_expr:0 typed_core_pattern:1 summary_only_pattern:0 typed_core_type:2 summary_only_type:0",
                "checked_preservation_expr:4 checked_preservation_pattern:1 checked_preservation_expr_structural:4 checked_preservation_pattern_structural:1",
            ],
        },
        ContractBaseline {
            module_name: "phase_list_cons",
            required_snippets: &[
                "body=ListCons:core=ListCons(Var(head)|Var(tail)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=ListCons(Var(head)|Var(tail))):proof=lean-covered",
                "children=[Var:core=Var(head):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(head)):proof=lean-covered:text=head:arity=0;Var:core=Var(tail):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(tail)):proof=lean-covered:text=tail:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:3 summary_only_expr:0 typed_core_pattern:2 summary_only_pattern:0 typed_core_type:3 summary_only_type:0",
                "checked_preservation_expr:3 checked_preservation_pattern:2 checked_preservation_expr_structural:3 checked_preservation_pattern_structural:2",
            ],
        },
        ContractBaseline {
            module_name: "phase_if_expr",
            required_snippets: &[
                "body=If:core=If(Var(flag)=>Int(1)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=If(Var(flag)=>Int(1))):proof=lean-covered",
                "children=[Var:core=Var(flag):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(flag)):proof=lean-covered:text=flag:arity=0;Int:core=Int(1):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Int(1)):proof=lean-covered:text=1:arity=0]",
                "metadata=functions:1 types:0 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:3 summary_only_expr:0 typed_core_pattern:1 summary_only_pattern:0 typed_core_type:2 summary_only_type:0",
                "checked_preservation_expr:3 checked_preservation_pattern:1 checked_preservation_expr_structural:3 checked_preservation_pattern_structural:1",
            ],
        },
        ContractBaseline {
            module_name: "phase_field_access",
            required_snippets: &[
                "type=Point visibility=Public params= body= body_core=Struct(Point;x:Int)",
                "body=FieldAccess:core=FieldAccess(Var(point).x):preservation=structural-core-expr(freshness=no-runtime-bindings;target=FieldAccess(Var(point).x)):proof=lean-covered",
                "children=[Var:core=Var(point):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(point)):proof=lean-covered:text=point:arity=0]",
                "metadata=functions:1 types:1 constructors:0 proof_readiness:lean-covered",
                "typed_core_expr:2 summary_only_expr:0 typed_core_pattern:1 summary_only_pattern:0 typed_core_type:3 summary_only_type:0",
                "checked_preservation_expr:2 checked_preservation_pattern:1 checked_preservation_expr_structural:2 checked_preservation_pattern_structural:1",
            ],
        },
    ]
}

/// Returns production CoreIR forms selected as the next Lean model candidates.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static slice of compiler fixtures and required CoreIR snippets for typed
///   forms that are intentionally still marked `proof-model-required`.
///
/// Transformation:
/// - Exposes immutable candidate expectations without running the compiler.
///   These baselines keep Rust CoreIR payloads stable before Lean syntax,
///   typing, and theorem support are added.
pub(crate) const fn next_lean_model_candidate_baselines() -> &'static [ContractBaseline] {
    &[ContractBaseline {
        module_name: "phase_trait",
        required_snippets: &[
            "body=Call:core=RemoteCall(Eq:equal;Var(Left),Var(Right)):preservation=structural-core-expr(freshness=no-runtime-bindings;target=RemoteCall(Eq:equal;Var(Left),Var(Right))):proof=proof-model-required:remote=Eq",
            "children=[Atom:core=Atom(equal):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Atom(equal)):proof=lean-covered:text=equal:arity=0;Var:core=Var(Left):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(Left)):proof=lean-covered:text=Left:arity=0;Var:core=Var(Right):preservation=structural-core-expr(freshness=no-runtime-bindings;target=Var(Right)):proof=lean-covered:text=Right:arity=0]",
            "metadata=functions:1 types:0 constructors:0 proof_readiness:proof-model-required",
            "typed_core_expr:4 summary_only_expr:0 typed_core_pattern:2 summary_only_pattern:0 typed_core_type:3 summary_only_type:0",
            "checked_preservation_expr:4 checked_preservation_pattern:2 checked_preservation_expr_structural:4 checked_preservation_pattern_structural:2",
        ],
    }]
}
