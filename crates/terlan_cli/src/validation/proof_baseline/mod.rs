/// Static CoreIR contract expectations for a gate-backed LP8 compiler fixture.
///
/// Inputs:
/// - `module_name`: phase-contract fixture module name used to locate the
///   source fixture.
/// - `required_snippets`: CoreIR contract fragments that must appear in the
///   fixture's lowered contract text.
///
/// Output:
/// - Immutable record consumed by formal CLI tests and future proof-export
///   preflight checks.
///
/// Transformation:
/// - Stores expected proof evidence without reading files or executing compiler
///   phases.
pub(crate) struct ContractBaseline {
    pub(crate) module_name: &'static str,
    pub(crate) required_snippets: &'static [&'static str],
}

/// Static phase-manifest counter expectations for a gate-backed LP8 fixture.
///
/// Inputs:
/// - `module_name`: phase-contract fixture module name used to locate the
///   source fixture.
/// - `counts`: expected numeric `core_proof_coverage` counters emitted by
///   `terlc check --emit-phase-manifest`.
///
/// Output:
/// - Immutable record consumed by formal CLI tests and future proof-export
///   preflight checks.
///
/// Transformation:
/// - Stores expected manifest proof counters without serializing or decoding
///   JSON.
pub(crate) struct ManifestBaseline {
    pub(crate) module_name: &'static str,
    pub(crate) counts: &'static [ManifestCount],
}

/// Expected numeric `core_proof_coverage` field for one manifest baseline.
///
/// Inputs:
/// - `field`: JSON field name under `core_proof_coverage`.
/// - `expected`: expected unsigned integer value for that field.
///
/// Output:
/// - Immutable field/value pair for manifest validation tests.
///
/// Transformation:
/// - Names one expected counter without reading the manifest.
pub(crate) struct ManifestCount {
    pub(crate) field: &'static str,
    pub(crate) expected: u64,
}

/// Builds one static manifest counter expectation.
///
/// Inputs:
/// - `field`: JSON field name under `core_proof_coverage`.
/// - `expected`: expected unsigned integer value for that field.
///
/// Output:
/// - `ManifestCount` with the provided field and expected value.
///
/// Transformation:
/// - Wraps the field/value pair in the baseline counter type without allocation.
const fn count(field: &'static str, expected: u64) -> ManifestCount {
    ManifestCount { field, expected }
}

/// Names every unresolved-constructor manifest counter required to be zero.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Static field-name slice for call, chain, and pattern unresolved
///   constructor counters.
///
/// Transformation:
/// - Centralizes the constructor-resolution field list used by baseline-shape
///   tests without allocating or inspecting manifest artifacts.
const UNRESOLVED_CONSTRUCTOR_COUNTER_FIELDS: &[&str] = &[
    "unresolved_constructor_call_candidate",
    "unresolved_constructor_chain_candidate",
    "unresolved_constructor_pattern_candidate",
];

/// Names every resolved-constructor manifest counter required by baselines.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Static field-name slice for call, chain, and pattern resolved constructor
///   identity counters.
///
/// Transformation:
/// - Centralizes the constructor-identity field list used by baseline-shape
///   tests without allocating or inspecting manifest artifacts.
const RESOLVED_CONSTRUCTOR_COUNTER_FIELDS: &[&str] = &[
    "resolved_constructor_call_identity",
    "resolved_constructor_chain_identity",
    "resolved_constructor_pattern_identity",
];

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

const PHASE_BASIC_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 2),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 2),
    count("summary_only_pattern", 0),
    count("typed_core_type", 3),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 2),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 2),
    count("checked_preservation_expr_no_runtime_bindings", 3),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 2),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_CORE_LEAN_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 1),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 1),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 1),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 1),
    count("summary_only_pattern", 0),
    count("typed_core_type", 2),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 1),
    count("checked_preservation_pattern", 1),
    count("checked_preservation_expr_structural", 1),
    count("checked_preservation_pattern_structural", 1),
    count("checked_preservation_expr_no_runtime_bindings", 1),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 1),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_ATOM_LITERAL_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 1),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 0),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 1),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 0),
    count("summary_only_pattern", 0),
    count("typed_core_type", 1),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 1),
    count("checked_preservation_pattern", 0),
    count("checked_preservation_expr_structural", 1),
    count("checked_preservation_pattern_structural", 0),
    count("checked_preservation_expr_no_runtime_bindings", 1),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 0),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_INT_LITERAL_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 1),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 0),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 1),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 0),
    count("summary_only_pattern", 0),
    count("typed_core_type", 1),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 1),
    count("checked_preservation_pattern", 0),
    count("checked_preservation_expr_structural", 1),
    count("checked_preservation_pattern_structural", 0),
    count("checked_preservation_expr_no_runtime_bindings", 1),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 0),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_CORE_LAMBDA_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 2),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 0),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 2),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 0),
    count("summary_only_pattern", 0),
    count("typed_core_type", 1),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 2),
    count("checked_preservation_pattern", 0),
    count("checked_preservation_expr_structural", 2),
    count("checked_preservation_pattern_structural", 0),
    count("checked_preservation_expr_no_runtime_bindings", 1),
    count("checked_preservation_expr_runtime_bindings_required", 1),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 0),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_CONSTRUCTOR_RESOLUTION_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 0),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 0),
    count("summary_only_pattern", 0),
    count("typed_core_type", 3),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 0),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 0),
    count("checked_preservation_expr_no_runtime_bindings", 3),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 0),
    count("resolved_constructor_call_identity", 1),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_CONSTRUCTOR_PATTERN_RESOLUTION_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 1),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 1),
    count("summary_only_pattern", 0),
    count("typed_core_type", 4),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 1),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 1),
    count("checked_preservation_expr_no_runtime_bindings", 2),
    count("checked_preservation_expr_runtime_bindings_required", 1),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 1),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 1),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_LITERAL_PATTERN_CASE_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 4),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 1),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 4),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 1),
    count("summary_only_pattern", 0),
    count("typed_core_type", 2),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 4),
    count("checked_preservation_pattern", 1),
    count("checked_preservation_expr_structural", 4),
    count("checked_preservation_pattern_structural", 1),
    count("checked_preservation_expr_no_runtime_bindings", 4),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 1),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_UNARY_OPERATOR_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 2),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 1),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 2),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 1),
    count("summary_only_pattern", 0),
    count("typed_core_type", 2),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 2),
    count("checked_preservation_pattern", 1),
    count("checked_preservation_expr_structural", 2),
    count("checked_preservation_pattern_structural", 1),
    count("checked_preservation_expr_no_runtime_bindings", 2),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 1),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_LIST_CONS_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 2),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 2),
    count("summary_only_pattern", 0),
    count("typed_core_type", 3),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 2),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 2),
    count("checked_preservation_expr_no_runtime_bindings", 3),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 2),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_IF_EXPR_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 1),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 1),
    count("summary_only_pattern", 0),
    count("typed_core_type", 2),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 1),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 1),
    count("checked_preservation_expr_no_runtime_bindings", 3),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 1),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_FIELD_ACCESS_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 2),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 1),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 2),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 1),
    count("summary_only_pattern", 0),
    count("typed_core_type", 3),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 2),
    count("checked_preservation_pattern", 1),
    count("checked_preservation_expr_structural", 2),
    count("checked_preservation_pattern_structural", 1),
    count("checked_preservation_expr_no_runtime_bindings", 2),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 1),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_BINARY_LITERAL_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 1),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 0),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 1),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 0),
    count("summary_only_pattern", 0),
    count("typed_core_type", 1),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 1),
    count("checked_preservation_pattern", 0),
    count("checked_preservation_expr_structural", 1),
    count("checked_preservation_pattern_structural", 0),
    count("checked_preservation_expr_no_runtime_bindings", 1),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 0),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_TUPLE_LITERAL_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 0),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 0),
    count("summary_only_pattern", 0),
    count("typed_core_type", 1),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 0),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 0),
    count("checked_preservation_expr_no_runtime_bindings", 3),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 0),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_LIST_LITERAL_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 0),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 0),
    count("summary_only_pattern", 0),
    count("typed_core_type", 1),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 0),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 0),
    count("checked_preservation_expr_no_runtime_bindings", 3),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 0),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_NAMED_CALL_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 4),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 1),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 4),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 1),
    count("summary_only_pattern", 0),
    count("typed_core_type", 3),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 4),
    count("checked_preservation_pattern", 1),
    count("checked_preservation_expr_structural", 4),
    count("checked_preservation_pattern_structural", 1),
    count("checked_preservation_expr_no_runtime_bindings", 4),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 1),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_BINARY_EQ_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 2),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 2),
    count("summary_only_pattern", 0),
    count("typed_core_type", 3),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 2),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 2),
    count("checked_preservation_expr_no_runtime_bindings", 3),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 2),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_BINARY_LT_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 2),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 2),
    count("summary_only_pattern", 0),
    count("typed_core_type", 3),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 2),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 2),
    count("checked_preservation_expr_no_runtime_bindings", 3),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 2),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_BINARY_LTE_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 2),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 2),
    count("summary_only_pattern", 0),
    count("typed_core_type", 3),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 2),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 2),
    count("checked_preservation_expr_no_runtime_bindings", 3),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 2),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_BINARY_GT_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 2),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 2),
    count("summary_only_pattern", 0),
    count("typed_core_type", 3),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 2),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 2),
    count("checked_preservation_expr_no_runtime_bindings", 3),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 2),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_BINARY_GTE_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 2),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 2),
    count("summary_only_pattern", 0),
    count("typed_core_type", 3),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 2),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 2),
    count("checked_preservation_expr_no_runtime_bindings", 3),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 2),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_BINARY_MUL_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 2),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 2),
    count("summary_only_pattern", 0),
    count("typed_core_type", 3),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 2),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 2),
    count("checked_preservation_expr_no_runtime_bindings", 3),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 2),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_BINARY_SUB_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 0),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 2),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 3),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 2),
    count("summary_only_pattern", 0),
    count("typed_core_type", 3),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 3),
    count("checked_preservation_pattern", 2),
    count("checked_preservation_expr_structural", 3),
    count("checked_preservation_pattern_structural", 2),
    count("checked_preservation_expr_no_runtime_bindings", 3),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 2),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const PHASE_TRAIT_MANIFEST_COUNTS: &[ManifestCount] = &[
    count("lean_covered", 3),
    count("partial", 0),
    count("proof_model_required", 1),
    count("runtime_boundary", 0),
    count("artifact_only", 0),
    count("pattern_lean_covered", 2),
    count("pattern_partial", 0),
    count("pattern_proof_model_required", 0),
    count("pattern_runtime_boundary", 0),
    count("pattern_artifact_only", 0),
    count("typed_core_expr", 4),
    count("summary_only_expr", 0),
    count("typed_core_pattern", 2),
    count("summary_only_pattern", 0),
    count("typed_core_type", 3),
    count("summary_only_type", 0),
    count("checked_preservation_expr", 4),
    count("checked_preservation_pattern", 2),
    count("checked_preservation_expr_structural", 4),
    count("checked_preservation_pattern_structural", 2),
    count("checked_preservation_expr_no_runtime_bindings", 4),
    count("checked_preservation_expr_runtime_bindings_required", 0),
    count("checked_preservation_pattern_no_runtime_bindings", 0),
    count("checked_preservation_pattern_runtime_bindings_required", 2),
    count("resolved_constructor_call_identity", 0),
    count("resolved_constructor_chain_identity", 0),
    count("resolved_constructor_pattern_identity", 0),
    count("unresolved_constructor_call_candidate", 0),
    count("unresolved_constructor_chain_candidate", 0),
    count("unresolved_constructor_pattern_candidate", 0),
];

const MANIFEST_BASELINES: &[ManifestBaseline] = &[
    ManifestBaseline {
        module_name: "phase_basic",
        counts: PHASE_BASIC_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_binary_eq",
        counts: PHASE_BINARY_EQ_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_binary_lt",
        counts: PHASE_BINARY_LT_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_binary_lte",
        counts: PHASE_BINARY_LTE_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_binary_gt",
        counts: PHASE_BINARY_GT_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_binary_gte",
        counts: PHASE_BINARY_GTE_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_binary_mul",
        counts: PHASE_BINARY_MUL_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_binary_sub",
        counts: PHASE_BINARY_SUB_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_core_lean",
        counts: PHASE_CORE_LEAN_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_int_literal",
        counts: PHASE_INT_LITERAL_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_atom_literal",
        counts: PHASE_ATOM_LITERAL_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_binary_literal",
        counts: PHASE_BINARY_LITERAL_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_tuple_literal",
        counts: PHASE_TUPLE_LITERAL_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_list_literal",
        counts: PHASE_LIST_LITERAL_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_named_call",
        counts: PHASE_NAMED_CALL_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_unary_operator",
        counts: PHASE_UNARY_OPERATOR_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_core_lambda",
        counts: PHASE_CORE_LAMBDA_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_constructor_resolution",
        counts: PHASE_CONSTRUCTOR_RESOLUTION_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_constructor_pattern_resolution",
        counts: PHASE_CONSTRUCTOR_PATTERN_RESOLUTION_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_literal_pattern_case",
        counts: PHASE_LITERAL_PATTERN_CASE_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_list_cons",
        counts: PHASE_LIST_CONS_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_if_expr",
        counts: PHASE_IF_EXPR_MANIFEST_COUNTS,
    },
    ManifestBaseline {
        module_name: "phase_field_access",
        counts: PHASE_FIELD_ACCESS_MANIFEST_COUNTS,
    },
];

const NEXT_MODEL_CANDIDATE_MANIFEST_BASELINES: &[ManifestBaseline] = &[ManifestBaseline {
    module_name: "phase_trait",
    counts: PHASE_TRAIT_MANIFEST_COUNTS,
}];

/// Returns the gate-backed phase-manifest baseline table.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static slice of LP8 compiler fixtures and their expected
///   `core_proof_coverage` counters.
///
/// Transformation:
/// - Exposes immutable proof-counter expectations for callers that already know
///   how to run `check --emit-phase-manifest` and decode JSON.
pub(crate) const fn manifest_baselines() -> &'static [ManifestBaseline] {
    MANIFEST_BASELINES
}

/// Returns phase-manifest baselines for the next Lean model candidates.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Static slice of compiler fixtures and expected `core_proof_coverage`
///   counters for typed forms that are intentionally not Lean-covered yet.
///
/// Transformation:
/// - Exposes immutable candidate proof-counter expectations for callers that
///   already know how to run `check --emit-phase-manifest` and decode JSON.
pub(crate) const fn next_lean_model_candidate_manifest_baselines() -> &'static [ManifestBaseline] {
    NEXT_MODEL_CANDIDATE_MANIFEST_BASELINES
}

/// Validates CoreIR contract text against one gate-backed baseline.
///
/// Inputs:
/// - `baseline`: static fixture contract baseline.
/// - `contract_text`: actual CoreIR contract text emitted by compiler lowering.
///
/// Output:
/// - `Ok(())` when every required snippet is present.
/// - `Err(String)` naming the missing snippet and fixture when validation
///   fails.
///
/// Transformation:
/// - Scans the actual contract text for each static required snippet without
///   mutating inputs or reading additional files.
pub(crate) fn validate_contract_baseline(
    baseline: &ContractBaseline,
    contract_text: &str,
) -> Result<(), String> {
    for expected in baseline.required_snippets {
        if !contract_text.contains(expected) {
            return Err(format!(
                "CoreIR contract for {} did not contain {expected:?}",
                baseline.module_name
            ));
        }
    }
    Ok(())
}

/// Validates manifest proof counters against one gate-backed baseline.
///
/// Inputs:
/// - `baseline`: static fixture manifest baseline.
/// - `count_for`: lookup function that returns the actual manifest value for a
///   `core_proof_coverage` field.
///
/// Output:
/// - `Ok(())` when every required counter is present and equal.
/// - `Err(String)` naming the missing or mismatched counter when validation
///   fails.
///
/// Transformation:
/// - Pulls actual counter values through `count_for` and compares them to the
///   static baseline without owning any JSON representation.
pub(crate) fn validate_manifest_baseline_counts(
    baseline: &ManifestBaseline,
    mut count_for: impl FnMut(&str) -> Option<u64>,
) -> Result<(), String> {
    for count in baseline.counts {
        let actual = count_for(count.field).ok_or_else(|| {
            format!(
                "manifest count {}.{} is missing",
                baseline.module_name, count.field
            )
        })?;
        if actual != count.expected {
            return Err(format!(
                "unexpected manifest count for {}.{}: expected {}, got {}",
                baseline.module_name, count.field, count.expected, actual
            ));
        }
    }
    Ok(())
}

/// Validates a phase manifest artifact against one gate-backed baseline and readiness.
///
/// Inputs:
/// - `baseline`: static fixture manifest baseline.
/// - `expected_readiness`: required manifest `core_proof_coverage.readiness`
///   value.
/// - `core_ir_hash`: actual manifest `core_ir_hash` value.
/// - `readiness`: actual manifest `core_proof_coverage.readiness` value.
/// - `count_for`: lookup function that returns the actual manifest value for a
///   `core_proof_coverage` field.
///
/// Output:
/// - `Ok(())` when the manifest has a nonzero CoreIR hash, reports the
///   expected readiness, and matches all baseline counters.
/// - `Err(String)` naming the failed artifact-level or counter-level
///   requirement.
///
/// Transformation:
/// - Checks artifact identity/readiness first, then delegates numeric counter
///   validation to `validate_manifest_baseline_counts`.
pub(crate) fn validate_manifest_baseline_artifact_with_readiness(
    baseline: &ManifestBaseline,
    expected_readiness: &str,
    core_ir_hash: Option<u64>,
    readiness: Option<&str>,
    count_for: impl FnMut(&str) -> Option<u64>,
) -> Result<(), String> {
    match core_ir_hash {
        Some(hash) if hash != 0 => {}
        Some(_) => {
            return Err(format!(
                "manifest for {} has zero core_ir_hash",
                baseline.module_name
            ));
        }
        None => {
            return Err(format!(
                "manifest for {} is missing core_ir_hash",
                baseline.module_name
            ));
        }
    }

    match readiness {
        Some(actual) if actual == expected_readiness => {}
        Some(actual) => {
            return Err(format!(
                "manifest for {} has readiness {actual:?}, expected {expected_readiness:?}",
                baseline.module_name,
            ));
        }
        None => {
            return Err(format!(
                "manifest for {} is missing core proof readiness",
                baseline.module_name
            ));
        }
    }

    validate_manifest_baseline_counts(baseline, count_for)
}

/// Validates a Lean-covered phase manifest artifact against one baseline.
///
/// Inputs:
/// - `baseline`: static fixture manifest baseline.
/// - `core_ir_hash`: actual manifest `core_ir_hash` value.
/// - `readiness`: actual manifest `core_proof_coverage.readiness` value.
/// - `count_for`: lookup function that returns the actual manifest value for a
///   `core_proof_coverage` field.
///
/// Output:
/// - `Ok(())` when the manifest has nonzero CoreIR hash, `lean-covered`
///   readiness, and matching counters.
/// - `Err(String)` naming the failed artifact-level or counter-level
///   requirement.
///
/// Transformation:
/// - Delegates to `validate_manifest_baseline_artifact_with_readiness` with the
///   Lean-ready readiness label.
pub(crate) fn validate_manifest_baseline_artifact(
    baseline: &ManifestBaseline,
    core_ir_hash: Option<u64>,
    readiness: Option<&str>,
    count_for: impl FnMut(&str) -> Option<u64>,
) -> Result<(), String> {
    validate_manifest_baseline_artifact_with_readiness(
        baseline,
        "lean-covered",
        core_ir_hash,
        readiness,
        count_for,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        contract_baselines, manifest_baselines, next_lean_model_candidate_baselines,
        next_lean_model_candidate_manifest_baselines, validate_contract_baseline,
        validate_manifest_baseline_artifact, validate_manifest_baseline_counts,
        RESOLVED_CONSTRUCTOR_COUNTER_FIELDS, UNRESOLVED_CONSTRUCTOR_COUNTER_FIELDS,
    };

    /// Verifies contract and manifest baselines protect the same LP8 fixtures.
    ///
    /// Inputs:
    /// - Static `contract_baselines` and `manifest_baselines` tables.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are modified.
    ///
    /// Transformation:
    /// - Extracts module names from both static tables and compares them to the
    ///   expected gate-backed LP8 fixture sequence.
    #[test]
    fn proof_baseline_tables_cover_same_fixture_set() {
        let contract_names = contract_baselines()
            .iter()
            .map(|baseline| baseline.module_name)
            .collect::<Vec<_>>();
        let manifest_names = manifest_baselines()
            .iter()
            .map(|baseline| baseline.module_name)
            .collect::<Vec<_>>();
        let expected = vec![
            "phase_basic",
            "phase_binary_eq",
            "phase_binary_lt",
            "phase_binary_lte",
            "phase_binary_gt",
            "phase_binary_gte",
            "phase_binary_mul",
            "phase_binary_sub",
            "phase_core_lean",
            "phase_int_literal",
            "phase_atom_literal",
            "phase_binary_literal",
            "phase_tuple_literal",
            "phase_list_literal",
            "phase_named_call",
            "phase_unary_operator",
            "phase_core_lambda",
            "phase_constructor_resolution",
            "phase_constructor_pattern_resolution",
            "phase_literal_pattern_case",
            "phase_list_cons",
            "phase_if_expr",
            "phase_field_access",
        ];

        assert_eq!(contract_names, expected);
        assert_eq!(manifest_names, expected);
    }

    /// Verifies candidate contract and manifest baselines protect the same fixtures.
    ///
    /// Inputs:
    /// - Static `next_lean_model_candidate_baselines` table.
    /// - Static `next_lean_model_candidate_manifest_baselines` table.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are modified.
    ///
    /// Transformation:
    /// - Extracts module names from both next-model candidate tables and checks
    ///   that the contract and manifest candidate lists stay aligned.
    #[test]
    fn proof_baseline_next_model_candidate_tables_cover_same_fixture_set() {
        let contract_names = next_lean_model_candidate_baselines()
            .iter()
            .map(|baseline| baseline.module_name)
            .collect::<Vec<_>>();
        let manifest_names = next_lean_model_candidate_manifest_baselines()
            .iter()
            .map(|baseline| baseline.module_name)
            .collect::<Vec<_>>();

        assert_eq!(contract_names, manifest_names);
    }

    /// Verifies candidate blocked-form snippets match manifest debt counters.
    ///
    /// Inputs:
    /// - Static `next_lean_model_candidate_baselines` table.
    /// - Static `next_lean_model_candidate_manifest_baselines` table.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are modified.
    ///
    /// Transformation:
    /// - Counts concrete `:proof=proof-model-required` expression snippets in
    ///   each next-model candidate contract baseline, then checks the matching
    ///   manifest baseline reports the same `proof_model_required` count.
    #[test]
    fn proof_baseline_next_model_candidate_blocked_form_counts_match_manifest() {
        for contract_candidate in next_lean_model_candidate_baselines() {
            let manifest_candidate = next_lean_model_candidate_manifest_baselines()
                .iter()
                .find(|candidate| candidate.module_name == contract_candidate.module_name)
                .unwrap_or_else(|| {
                    panic!(
                        "{} missing next-model candidate manifest baseline",
                        contract_candidate.module_name
                    )
                });
            let contract_blocked_form_count = contract_candidate
                .required_snippets
                .iter()
                .filter(|snippet| snippet.contains(":proof=proof-model-required"))
                .count() as u64;
            let manifest_blocked_form_count = manifest_candidate
                .counts
                .iter()
                .find(|count| count.field == "proof_model_required")
                .unwrap_or_else(|| {
                    panic!(
                        "{} missing proof_model_required manifest counter",
                        manifest_candidate.module_name
                    )
                })
                .expected;

            assert_eq!(
                contract_blocked_form_count, manifest_blocked_form_count,
                "{} contract blocked-form snippets must match manifest proof_model_required count",
                contract_candidate.module_name
            );
        }
    }

    /// Verifies the Lean resume cycle has exactly one pinned candidate.
    ///
    /// Inputs:
    /// - Static `next_lean_model_candidate_baselines` table.
    /// - Static `next_lean_model_candidate_manifest_baselines` table.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are modified.
    ///
    /// Transformation:
    /// - Counts next-model candidate contract and manifest entries and checks
    ///   that each table contains one selected fixture for the current cycle.
    #[test]
    fn proof_baseline_next_model_candidates_select_exactly_one_fixture() {
        assert_eq!(
            next_lean_model_candidate_baselines().len(),
            1,
            "LP8 should select exactly one next-model contract candidate"
        );
        assert_eq!(
            next_lean_model_candidate_manifest_baselines().len(),
            1,
            "LP8 should select exactly one next-model manifest candidate"
        );
    }

    /// Verifies next-model candidates are not mixed into Lean-ready baselines.
    ///
    /// Inputs:
    /// - Static Lean-ready and next-model candidate baseline tables.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are modified.
    ///
    /// Transformation:
    /// - Checks candidate snippets explicitly require `proof-model-required`
    ///   readiness plus at least one concrete proof-model-required contract
    ///   form, and that no candidate fixture is already in the Lean-ready
    ///   baseline table.
    #[test]
    fn proof_baseline_next_model_candidates_are_separate_from_ready_baselines() {
        for candidate in next_lean_model_candidate_baselines() {
            assert!(
                candidate
                    .required_snippets
                    .iter()
                    .any(|snippet| snippet.contains("proof_readiness:proof-model-required")),
                "{} must remain a proof-model-required candidate",
                candidate.module_name
            );
            assert!(
                candidate
                    .required_snippets
                    .iter()
                    .any(|snippet| snippet.contains(":proof=proof-model-required")),
                "{} must pin the concrete proof-model-required Core form",
                candidate.module_name
            );
            assert!(
                !contract_baselines()
                    .iter()
                    .any(|baseline| baseline.module_name == candidate.module_name),
                "{} must not be listed as Lean-ready yet",
                candidate.module_name
            );
        }

        for candidate in next_lean_model_candidate_manifest_baselines() {
            let proof_model_count = candidate
                .counts
                .iter()
                .find(|count| count.field == "proof_model_required")
                .unwrap_or_else(|| {
                    panic!("{} missing proof_model_required", candidate.module_name)
                });
            assert!(
                proof_model_count.expected > 0,
                "{} must carry explicit proof-model-required debt",
                candidate.module_name
            );
            assert!(
                !manifest_baselines()
                    .iter()
                    .any(|baseline| baseline.module_name == candidate.module_name),
                "{} must not be listed as Lean-ready manifest baseline yet",
                candidate.module_name
            );
        }
    }

    /// Verifies CoreIR contract baselines include readiness-critical snippets.
    ///
    /// Inputs:
    /// - Static `contract_baselines` table.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are modified.
    ///
    /// Transformation:
    /// - Checks each baseline has nonempty snippets and explicitly requires
    ///   `proof_readiness:lean-covered` in the expected CoreIR contract text.
    #[test]
    fn proof_baseline_contracts_require_lean_covered_readiness() {
        for baseline in contract_baselines() {
            assert!(
                !baseline.required_snippets.is_empty(),
                "{} must require CoreIR snippets",
                baseline.module_name
            );
            assert!(
                baseline
                    .required_snippets
                    .iter()
                    .any(|snippet| snippet.contains("proof_readiness:lean-covered")),
                "{} must require lean-covered metadata readiness",
                baseline.module_name
            );
        }
    }

    /// Verifies phase-manifest baselines include proof-export safety counters.
    ///
    /// Inputs:
    /// - Static `manifest_baselines` table.
    /// - Static `next_lean_model_candidate_manifest_baselines` table.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are modified.
    ///
    /// Transformation:
    /// - Checks every Lean-ready and next-model candidate manifest baseline has
    ///   nonempty counters, requires zero unresolved constructor call, chain,
    ///   and pattern candidates, and records both expression freshness
    ///   partitions.
    /// - Checks Lean-ready baselines, but not next-model candidates, require
    ///   zero proof-readiness debt counters.
    #[test]
    fn proof_baseline_manifests_require_resolution_and_freshness_counters() {
        for baseline in manifest_baselines()
            .iter()
            .chain(next_lean_model_candidate_manifest_baselines())
        {
            assert!(
                !baseline.counts.is_empty(),
                "{} must require manifest counters",
                baseline.module_name
            );
            for field in [
                "partial",
                "proof_model_required",
                "runtime_boundary",
                "artifact_only",
                "pattern_partial",
                "pattern_proof_model_required",
                "pattern_runtime_boundary",
                "pattern_artifact_only",
                "checked_preservation_expr_structural",
                "checked_preservation_pattern_structural",
                "checked_preservation_expr_no_runtime_bindings",
                "checked_preservation_expr_runtime_bindings_required",
                "checked_preservation_pattern_no_runtime_bindings",
                "checked_preservation_pattern_runtime_bindings_required",
            ] {
                assert!(
                    baseline.counts.iter().any(|count| count.field == field),
                    "{} must require {field}",
                    baseline.module_name
                );
            }
            for field in RESOLVED_CONSTRUCTOR_COUNTER_FIELDS
                .iter()
                .chain(UNRESOLVED_CONSTRUCTOR_COUNTER_FIELDS)
            {
                assert!(
                    baseline.counts.iter().any(|count| count.field == *field),
                    "{} must require {field}",
                    baseline.module_name
                );
            }
            for field in UNRESOLVED_CONSTRUCTOR_COUNTER_FIELDS {
                let count = baseline
                    .counts
                    .iter()
                    .find(|count| count.field == *field)
                    .unwrap_or_else(|| panic!("{} missing {field}", baseline.module_name));
                assert_eq!(
                    count.expected, 0,
                    "{} must require zero {field}",
                    baseline.module_name
                );
            }
        }

        for baseline in manifest_baselines() {
            for field in [
                "partial",
                "proof_model_required",
                "runtime_boundary",
                "artifact_only",
                "pattern_partial",
                "pattern_proof_model_required",
                "pattern_runtime_boundary",
                "pattern_artifact_only",
            ] {
                let count = baseline
                    .counts
                    .iter()
                    .find(|count| count.field == field)
                    .unwrap_or_else(|| panic!("{} missing {field}", baseline.module_name));
                assert_eq!(
                    count.expected, 0,
                    "{} must require zero {field}",
                    baseline.module_name
                );
            }
        }
    }

    /// Verifies each manifest baseline has one expectation per field.
    ///
    /// Inputs:
    /// - Static `manifest_baselines` table.
    /// - Static `next_lean_model_candidate_manifest_baselines` table.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are modified.
    ///
    /// Transformation:
    /// - Compares each counter field with preceding fields in the same baseline
    ///   and fails if a static Lean-ready or next-model candidate manifest
    ///   contract repeats a field name.
    #[test]
    fn proof_baseline_manifest_fields_are_unique() {
        for baseline in manifest_baselines()
            .iter()
            .chain(next_lean_model_candidate_manifest_baselines())
        {
            for (index, count) in baseline.counts.iter().enumerate() {
                assert!(
                    !baseline.counts[..index]
                        .iter()
                        .any(|previous| previous.field == count.field),
                    "{} repeats manifest field {}",
                    baseline.module_name,
                    count.field
                );
            }
        }
    }

    /// Verifies the reusable contract validator reports missing snippets.
    ///
    /// Inputs:
    /// - First static contract baseline.
    /// - Deliberately empty CoreIR contract text.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are modified.
    ///
    /// Transformation:
    /// - Runs `validate_contract_baseline` against invalid text and checks that
    ///   the error identifies the affected fixture.
    #[test]
    fn proof_baseline_contract_validator_reports_missing_snippet() {
        let baseline = &contract_baselines()[0];
        let err =
            validate_contract_baseline(baseline, "").expect_err("empty contract text should fail");

        assert!(err.contains(baseline.module_name));
    }

    /// Verifies the reusable manifest validator reports counter mismatches.
    ///
    /// Inputs:
    /// - First static manifest baseline.
    /// - Lookup closure that returns zero for every requested counter.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are modified.
    ///
    /// Transformation:
    /// - Runs `validate_manifest_baseline_counts` against intentionally stale
    ///   counter values and checks that the error names the mismatched fixture.
    #[test]
    fn proof_baseline_manifest_validator_reports_counter_mismatch() {
        let baseline = &manifest_baselines()[0];
        let err = validate_manifest_baseline_counts(baseline, |_| Some(0))
            .expect_err("zeroed manifest counters should fail");

        assert!(err.contains(baseline.module_name));
    }

    /// Verifies the reusable manifest artifact validator reports bad readiness.
    ///
    /// Inputs:
    /// - First static manifest baseline.
    /// - Nonzero CoreIR hash.
    /// - Deliberately stale proof readiness value.
    /// - Lookup closure that returns expected baseline counters.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are modified.
    ///
    /// Transformation:
    /// - Runs `validate_manifest_baseline_artifact` against stale readiness and
    ///   checks that the error names the affected fixture.
    #[test]
    fn proof_baseline_manifest_artifact_validator_reports_bad_readiness() {
        let baseline = &manifest_baselines()[0];
        let err = validate_manifest_baseline_artifact(
            baseline,
            Some(1),
            Some("proof-model-required"),
            |field| {
                baseline
                    .counts
                    .iter()
                    .find(|count| count.field == field)
                    .map(|count| count.expected)
            },
        )
        .expect_err("stale readiness should fail");

        assert!(err.contains(baseline.module_name));
    }

    /// Verifies the pinned remote-call candidate keeps concrete compiler data.
    ///
    /// Inputs:
    /// - Static next-model candidate table.
    ///
    /// Output:
    /// - Test assertion only; no files or compiler artifacts are modified.
    ///
    /// Transformation:
    /// - Checks the static `phase_trait` baseline is still pinned to a
    ///   proof-model-required remote-call Core form. Documentation handoff
    ///   wording is validated by internal script tooling rather than crate
    ///   tests so release compiler crates do not include roadmap prose.
    #[test]
    fn proof_baseline_phase_trait_pins_remote_dispatch_contract() {
        let Some(phase_trait_candidate) = next_lean_model_candidate_baselines()
            .iter()
            .find(|candidate| candidate.module_name == "phase_trait")
        else {
            return;
        };

        assert!(
            phase_trait_candidate
                .required_snippets
                .iter()
                .any(|snippet| snippet.contains("RemoteCall(")
                    && snippet.contains(":proof=proof-model-required")),
            "phase_trait must remain pinned to a proof-model-required remote-call Core form"
        );
    }
}
