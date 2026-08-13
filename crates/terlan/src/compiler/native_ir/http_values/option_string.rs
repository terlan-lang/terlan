//! Shared direct-AOT case lowering for managed `Option[String]` values.

use crate::terlan_typeck::{
    CoreCaseClause, CoreExpr, CoreIfClause, CoreIntrinsicId, CoreLetBinding, CorePattern,
    CorePrimitiveIntrinsic, CoreType,
};

use super::MANAGED_HTTP_MODULE;

/// Eliminates one immediate match over a managed request lookup result.
pub(super) fn lower_request_option_case(
    scrutinee: &CoreExpr,
    clauses: &[CoreCaseClause],
) -> Result<Option<CoreExpr>, String> {
    if !is_request_string_option(scrutinee) {
        return Ok(None);
    }
    lower_managed_option_case(
        scrutinee,
        clauses,
        ManagedOptionCase {
            temporary: "$native_http_request_option",
            none_operation: "option_is_none",
            some_operation: "option_some",
            diagnostic: "native_ir.http_request_option_case",
        },
    )
    .map(Some)
}

/// Eliminates `Option.with_default` over one managed request lookup result.
pub(super) fn lower_request_option_default(
    scrutinee: &CoreExpr,
    default: CoreExpr,
) -> Result<Option<CoreExpr>, String> {
    if !is_request_string_option(scrutinee) {
        return Ok(None);
    }
    lower_managed_option_case(
        scrutinee,
        &[
            CoreCaseClause {
                pattern: CorePattern::Constructor {
                    name: "Some".to_string(),
                    constructor_identity: Some("std.core.Option.Some".to_string()),
                    args: vec![CorePattern::Var(
                        "$native_http_request_option_value".to_string(),
                    )],
                },
                guard: None,
                body: CoreExpr::Var("$native_http_request_option_value".to_string()),
            },
            CoreCaseClause {
                pattern: CorePattern::Wildcard,
                guard: None,
                body: default,
            },
        ],
        ManagedOptionCase {
            temporary: "$native_http_request_option",
            none_operation: "option_is_none",
            some_operation: "option_some",
            diagnostic: "native_ir.http_request_option_default",
        },
    )
    .map(Some)
}

/// Reports whether an expression returns the request boundary's `Option[String]`.
fn is_request_string_option(expr: &CoreExpr) -> bool {
    matches!(
        expr,
        CoreExpr::RemoteCall { module, function, args }
            if module == MANAGED_HTTP_MODULE
                && matches!(function.as_str(), "param" | "query" | "header" | "cookie" | "jar_get")
                && args.len() == 2
    ) || matches!(
        expr,
        CoreExpr::Intrinsic(call)
            if call.id == CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapGet)
                && matches!(
                    &call.return_type,
                    CoreType::Apply { constructor, args }
                        if constructor.rsplit('.').next() == Some("Option")
                            && matches!(args.as_slice(), [CoreType::String])
                )
    )
}

/// Compiler-private operation identities used by one managed option match.
pub(super) struct ManagedOptionCase {
    /// Stable temporary that retains the option across ordered branches.
    pub(super) temporary: &'static str,
    /// Managed predicate returning whether the active variant is `None`.
    pub(super) none_operation: &'static str,
    /// Managed projection returning the active `Some` string payload.
    pub(super) some_operation: &'static str,
    /// Stable diagnostic namespace for malformed checked patterns.
    pub(super) diagnostic: &'static str,
}

/// Eliminates one immediate `None`/`Some` match over a managed string option.
pub(super) fn lower_managed_option_case(
    scrutinee: &CoreExpr,
    clauses: &[CoreCaseClause],
    operations: ManagedOptionCase,
) -> Result<CoreExpr, String> {
    if clauses.is_empty() {
        return Err(diagnostic(
            operations.diagnostic,
            "managed option case has no clauses",
        ));
    }
    let branches = clauses
        .iter()
        .cloned()
        .map(|clause| lower_option_clause(clause, &operations))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var(operations.temporary.to_string()),
            value: scrutinee.clone(),
        }],
        body: Box::new(CoreExpr::If { clauses: branches }),
    })
}

/// Lowers one ordered option clause into a native Boolean branch.
fn lower_option_clause(
    clause: CoreCaseClause,
    operations: &ManagedOptionCase,
) -> Result<CoreIfClause, String> {
    let (condition, payload) = option_pattern(&clause.pattern, operations)?;
    let condition = match clause.guard {
        Some(guard) => CoreExpr::If {
            clauses: vec![
                CoreIfClause {
                    condition,
                    body: bind_option_payload(payload.as_deref(), guard, operations),
                },
                CoreIfClause {
                    condition: CoreExpr::Atom("true".to_string()),
                    body: CoreExpr::Atom("false".to_string()),
                },
            ],
        },
        None => condition,
    };
    Ok(CoreIfClause {
        condition,
        body: bind_option_payload(payload.as_deref(), clause.body, operations),
    })
}

/// Converts one supported option pattern into its predicate and payload name.
fn option_pattern(
    pattern: &CorePattern,
    operations: &ManagedOptionCase,
) -> Result<(CoreExpr, Option<String>), String> {
    match pattern {
        CorePattern::Atom(name) if name == "none" => Ok((option_is_none(operations), None)),
        CorePattern::Constructor { name, args, .. }
            if name.rsplit('.').next() == Some("None") && args.is_empty() =>
        {
            Ok((option_is_none(operations), None))
        }
        CorePattern::Constructor { name, args, .. }
            if name.rsplit('.').next() == Some("Some") && args.len() == 1 =>
        {
            let payload = match &args[0] {
                CorePattern::Wildcard => None,
                CorePattern::Var(name) => Some(name.clone()),
                _ => {
                    return Err(diagnostic(
                        operations.diagnostic,
                        "Some payload must be a variable or wildcard",
                    ));
                }
            };
            Ok((
                CoreExpr::UnaryOp {
                    operator: "not".to_string(),
                    operand: Box::new(option_is_none(operations)),
                },
                payload,
            ))
        }
        CorePattern::Tuple(items) if matches!(items.as_slice(), [CorePattern::Atom(tag), _] if tag == "some") =>
        {
            let [CorePattern::Atom(_), payload] = items.as_slice() else {
                unreachable!("matched structural option pattern")
            };
            let payload = match payload {
                CorePattern::Wildcard => None,
                CorePattern::Var(name) => Some(name.clone()),
                _ => {
                    return Err(diagnostic(
                        operations.diagnostic,
                        "Some payload must be a variable or wildcard",
                    ));
                }
            };
            Ok((
                CoreExpr::UnaryOp {
                    operator: "not".to_string(),
                    operand: Box::new(option_is_none(operations)),
                },
                payload,
            ))
        }
        CorePattern::Wildcard => Ok((CoreExpr::Atom("true".to_string()), None)),
        _ => Err(diagnostic(
            operations.diagnostic,
            "expected None, Some, or wildcard pattern",
        )),
    }
}

/// Builds the compiler-private managed `None` predicate.
fn option_is_none(operations: &ManagedOptionCase) -> CoreExpr {
    managed_call(
        operations.none_operation,
        vec![CoreExpr::Var(operations.temporary.to_string())],
    )
}

/// Introduces a branch-local `Some` payload when the pattern names it.
fn bind_option_payload(
    payload: Option<&str>,
    body: CoreExpr,
    operations: &ManagedOptionCase,
) -> CoreExpr {
    let Some(payload) = payload else {
        return body;
    };
    CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var(payload.to_string()),
            value: managed_call(
                operations.some_operation,
                vec![CoreExpr::Var(operations.temporary.to_string())],
            ),
        }],
        body: Box::new(body),
    }
}

/// Creates one compiler-private managed HTTP operation.
fn managed_call(function: &str, args: Vec<CoreExpr>) -> CoreExpr {
    CoreExpr::RemoteCall {
        module: MANAGED_HTTP_MODULE.to_string(),
        function: function.to_string(),
        args,
    }
}

/// Renders one stable managed-option diagnostic.
fn diagnostic(namespace: &str, message: &str) -> String {
    format!("error[{namespace}]: {message}")
}
