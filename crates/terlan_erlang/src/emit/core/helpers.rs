use super::super::erl::{ErlBinaryOp, ErlCaseClause, ErlExpr, ErlPattern};

/// Inputs:
/// - `module`: Terlan/CoreIR module name for the backend call.
/// - `function`: Erlang function name after any required sanitization.
/// - `args`: already-lowered Erlang argument expressions.
///
/// Output:
/// - Erlang remote-call expression.
///
/// Transformation:
/// - Stores the module/function/argument payload in the emitter AST and leaves
///   final module-name normalization to `ErlExpr::render`.
pub(super) fn erl_remote_call(module: &str, function: &str, args: Vec<ErlExpr>) -> ErlExpr {
    ErlExpr::Call {
        module: Some(module.to_string()),
        function: function.to_string(),
        args,
    }
}

/// Builds an Erlang exact-equality expression.
///
/// Inputs:
/// - `left`: left Erlang expression.
/// - `right`: right Erlang expression.
///
/// Output:
/// - Erlang binary operation using `=:=`.
///
/// Transformation:
/// - Wraps the two lowered operands in the emitter AST with the exact equality
///   operator used by runtime string result checks.
pub(super) fn erl_exact_eq(left: ErlExpr, right: ErlExpr) -> ErlExpr {
    ErlExpr::BinaryOp {
        op: ErlBinaryOp::EqEqEq,
        left: Box::new(left),
        right: Box::new(right),
    }
}

/// Builds a Terlan option `some` value in the Erlang backend representation.
///
/// Inputs:
/// - `value`: Erlang expression payload.
///
/// Output:
/// - Erlang tuple expression representing `some(value)`.
///
/// Transformation:
/// - Uses a tagged tuple so optional CoreIR results have an explicit runtime
///   shape in the Erlang backend.
pub(super) fn erl_some(value: ErlExpr) -> ErlExpr {
    ErlExpr::Tuple(vec![ErlExpr::Atom("some".to_string()), value])
}

/// Builds a Terlan option `none` value in the Erlang backend representation.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Erlang atom expression representing `none`.
///
/// Transformation:
/// - Uses the backend atom form chosen for CoreIR optional results.
pub(super) fn erl_none() -> ErlExpr {
    ErlExpr::Atom("none".to_string())
}

/// Builds a Terlan `Result.Ok` value in the Erlang backend representation.
///
/// Inputs:
/// - `value`: Erlang expression payload.
///
/// Output:
/// - Erlang tuple expression representing `Ok(value)`.
///
/// Transformation:
/// - Uses the same tagged tuple shape emitted for result constructors so
///   compiler-owned Task intrinsics compose with ordinary Result patterns.
pub(super) fn erl_result_ok(value: ErlExpr) -> ErlExpr {
    ErlExpr::Tuple(vec![ErlExpr::Atom("ok".to_string()), value])
}

/// Converts Erlang string search sentinel results into booleans.
///
/// Inputs:
/// - `scrutinee`: Erlang expression returning either `'nomatch'` or a match
///   payload.
///
/// Output:
/// - Erlang case expression returning `false` for `'nomatch'` and `true`
///   otherwise.
///
/// Transformation:
/// - Hides Erlang's search sentinel behind Terlan's boolean intrinsic contract.
pub(super) fn nomatch_case_to_bool(scrutinee: ErlExpr) -> ErlExpr {
    ErlExpr::Case {
        scrutinee: Box::new(scrutinee),
        clauses: vec![
            ErlCaseClause {
                pattern: ErlPattern::Atom("nomatch".to_string()),
                guard: None,
                body: ErlExpr::Atom("false".to_string()),
            },
            ErlCaseClause {
                pattern: ErlPattern::Wildcard,
                guard: None,
                body: ErlExpr::Atom("true".to_string()),
            },
        ],
    }
}

/// Validates exact intrinsic arity for vector arguments.
///
/// Inputs:
/// - `args`: lowered Erlang argument expressions.
/// - `expected`: required arity.
///
/// Output:
/// - `Some(args)` when `args.len() == expected`.
/// - `None` when the intrinsic call has malformed arity.
///
/// Transformation:
/// - Performs arity validation without mutating or reordering arguments.
pub(super) fn exact_args(args: Vec<ErlExpr>, expected: usize) -> Option<Vec<ErlExpr>> {
    (args.len() == expected).then_some(args)
}

/// Validates exact intrinsic arity and converts arguments into an array.
///
/// Inputs:
/// - `args`: lowered Erlang argument expressions.
///
/// Output:
/// - `Some([ErlExpr; N])` when the vector length matches `N`.
/// - `None` when the intrinsic call has malformed arity.
///
/// Transformation:
/// - Uses Rust's vector-to-array conversion so call sites can destructure
///   validated intrinsic arguments by position.
pub(super) fn exact_array_args<const N: usize>(args: Vec<ErlExpr>) -> Option<[ErlExpr; N]> {
    args.try_into().ok()
}
