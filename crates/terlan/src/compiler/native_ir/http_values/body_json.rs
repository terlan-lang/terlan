//! Managed request-body JSON and result contracts for direct AOT handlers.

use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_field_operation, encode_aggregate_layout, encode_json_parse_result_operation,
    encode_result_is_ok_operation, ManagedAggregateDescriptor, ManagedFieldType, SemanticTypeId,
};
use crate::terlan_typeck::{
    CoreCaseClause, CoreExpr, CoreIfClause, CoreLetBinding, CorePattern, CoreType,
};

use super::super::{NativeExpr, NativeType};
use super::{semantic, MANAGED_HTTP_MODULE};

const BODY_JSON_TEMPORARY: &str = "$native_http_body_json_result";

/// Returns the canonical managed semantic identity of `Json`.
pub(super) fn body_json_json_semantic() -> Result<SemanticTypeId, String> {
    semantic(&CoreType::Named("Json".to_string()).contract_text())
}

/// Returns the canonical managed semantic identity of the portable base error.
pub(super) fn body_json_error_semantic() -> Result<SemanticTypeId, String> {
    semantic(&CoreType::Named("Error".to_string()).contract_text())
}

/// Returns the canonical managed semantic identity of `Result[Json, Error]`.
pub(super) fn body_json_result_semantic() -> Result<SemanticTypeId, String> {
    semantic(&body_json_result_type().contract_text())
}

/// Returns the exact native result type of one compiler-private body operation.
pub(super) fn body_json_operation_type(expr: &CoreExpr) -> Option<NativeType> {
    let CoreExpr::RemoteCall {
        module,
        function,
        args,
    } = expr
    else {
        return None;
    };
    if module != MANAGED_HTTP_MODULE {
        return None;
    }
    match (function.as_str(), args.len()) {
        ("body_json", 1) => body_json_result_semantic().ok().map(NativeType::ManagedRef),
        ("body_json_is_ok", 1) => Some(NativeType::Bool),
        ("body_json_ok", 1) => body_json_json_semantic().ok().map(NativeType::ManagedRef),
        ("body_json_err", 1) => body_json_error_semantic().ok().map(NativeType::ManagedRef),
        _ => None,
    }
}

/// Lowers one compiler-private body operation into managed NativeIR.
pub(super) fn lower_managed_body_json_operation(
    function: &str,
    args: &[CoreExpr],
    lower: &mut impl FnMut(&CoreExpr) -> Result<NativeExpr, String>,
) -> Result<Option<NativeExpr>, String> {
    if function == "body_json_is_ok" && args.len() == 1 {
        return Ok(Some(NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_result_is_ok_operation(body_json_result_semantic()?)),
            args: args.iter().map(lower).collect::<Result<Vec<_>, _>>()?,
        }));
    }
    if matches!(function, "body_json_ok" | "body_json_err") && args.len() == 1 {
        return Ok(Some(NativeExpr::ManagedOperation {
            encoded: Arc::from(
                encode_aggregate_field_operation(body_json_result_semantic()?, 0)
                    .map_err(|error| format!("error[native_ir.http_body_json_result]: {error}"))?,
            ),
            args: args.iter().map(lower).collect::<Result<Vec<_>, _>>()?,
        }));
    }
    if function != "body_json" || args.len() != 1 {
        return Ok(None);
    }
    let request_semantic = semantic(&CoreType::Named("Request".to_string()).contract_text())?;
    let body = NativeExpr::ManagedOperation {
        encoded: Arc::from(
            encode_aggregate_field_operation(request_semantic, 4)
                .map_err(|error| format!("error[native_ir.http_body_json_project]: {error}"))?,
        ),
        args: vec![lower(&args[0])?],
    };
    Ok(Some(NativeExpr::ManagedOperation {
        encoded: Arc::from(encode_json_parse_result_operation(
            body_json_json_semantic()?,
            body_json_result_semantic()?,
            body_json_error_semantic()?,
        )),
        args: vec![body],
    }))
}

/// Builds all fixed layouts reachable from `Request.body_json()`.
pub(super) fn body_json_layouts() -> Result<Vec<Arc<[u8]>>, String> {
    let string = ManagedFieldType::Reference(semantic("std.core.String")?);
    let json = Arc::new(
        ManagedAggregateDescriptor::tuple(
            &CoreType::Named("Json".to_string()).contract_text(),
            vec![string],
        )
        .map_err(|error| format!("error[native_ir.http_body_json_layout]: {error}"))?,
    );
    let error = Arc::new(
        ManagedAggregateDescriptor::record(
            &CoreType::Named("Error".to_string()).contract_text(),
            vec![
                ("code".to_string(), ManagedFieldType::Atom),
                ("message".to_string(), string),
            ],
        )
        .map_err(|error| format!("error[native_ir.http_body_json_layout]: {error}"))?,
    );
    let result = body_json_result_type().contract_text();
    let ok = Arc::new(
        ManagedAggregateDescriptor::constructor(
            &result,
            "Ok",
            0,
            2,
            vec![(
                Some("value".to_string()),
                ManagedFieldType::Reference(body_json_json_semantic()?),
            )],
        )
        .map_err(|error| format!("error[native_ir.http_body_json_layout]: {error}"))?,
    );
    let err = Arc::new(
        ManagedAggregateDescriptor::constructor(
            &result,
            "Err",
            1,
            2,
            vec![(
                Some("reason".to_string()),
                ManagedFieldType::Reference(body_json_error_semantic()?),
            )],
        )
        .map_err(|error| format!("error[native_ir.http_body_json_layout]: {error}"))?,
    );
    [json, error, ok, err]
        .into_iter()
        .map(|descriptor| {
            encode_aggregate_layout(&descriptor)
                .map(Arc::from)
                .map_err(|error| format!("error[native_ir.http_body_json_abi]: {error}"))
        })
        .collect()
}

/// Eliminates one immediate `Ok`/`Err` match over `Request.body_json()`.
pub(super) fn lower_body_json_case(
    scrutinee: &CoreExpr,
    clauses: &[CoreCaseClause],
) -> Result<Option<CoreExpr>, String> {
    if !matches!(
        scrutinee,
        CoreExpr::RemoteCall { module, function, args }
            if module == MANAGED_HTTP_MODULE && function == "body_json" && args.len() == 1
    ) {
        return Ok(None);
    }
    if clauses.is_empty() {
        return Err(
            "error[native_ir.http_body_json_case]: body JSON case has no clauses".to_string(),
        );
    }
    let mut branches = Vec::with_capacity(clauses.len());
    for clause in clauses {
        branches.push(lower_result_clause(clause.clone())?);
    }
    Ok(Some(CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var(BODY_JSON_TEMPORARY.to_string()),
            value: scrutinee.clone(),
        }],
        body: Box::new(CoreExpr::If { clauses: branches }),
    }))
}

/// Lowers one ordered result clause into a boolean branch and payload binding.
fn lower_result_clause(clause: CoreCaseClause) -> Result<CoreIfClause, String> {
    let (condition, payload) = result_pattern(&clause.pattern)?;
    let condition = match clause.guard {
        Some(guard) => guarded_condition(condition, payload.as_ref(), guard),
        None => condition,
    };
    Ok(CoreIfClause {
        condition,
        body: bind_payload(payload.as_ref(), clause.body),
    })
}

/// Evaluates a payload-dependent guard only after its variant has matched.
fn guarded_condition(
    condition: CoreExpr,
    payload: Option<&PayloadBinding>,
    guard: CoreExpr,
) -> CoreExpr {
    CoreExpr::If {
        clauses: vec![
            CoreIfClause {
                condition,
                body: bind_payload(payload, guard),
            },
            CoreIfClause {
                condition: CoreExpr::Atom("true".to_string()),
                body: CoreExpr::Atom("false".to_string()),
            },
        ],
    }
}

/// Maps one supported result pattern to its predicate and optional payload binding.
fn result_pattern(pattern: &CorePattern) -> Result<(CoreExpr, Option<PayloadBinding>), String> {
    match pattern {
        CorePattern::Constructor { name, args, .. }
            if matches!(name.rsplit('.').next(), Some("Ok" | "Err")) && args.len() == 1 =>
        {
            let variant = name.rsplit('.').next().expect("matched result variant");
            let is_ok = managed_call(
                "body_json_is_ok",
                vec![CoreExpr::Var(BODY_JSON_TEMPORARY.to_string())],
            );
            let condition = if variant == "Ok" {
                is_ok
            } else {
                CoreExpr::UnaryOp {
                    operator: "not".to_string(),
                    operand: Box::new(is_ok),
                }
            };
            let payload = match &args[0] {
                CorePattern::Wildcard => None,
                CorePattern::Var(name) => Some(PayloadBinding {
                    name: name.clone(),
                    operation: if variant == "Ok" {
                        "body_json_ok"
                    } else {
                        "body_json_err"
                    },
                }),
                _ => {
                    return Err(
                        "error[native_ir.http_body_json_case]: result payload must be a variable or wildcard"
                            .to_string(),
                    )
                }
            };
            Ok((condition, payload))
        }
        CorePattern::Wildcard => Ok((CoreExpr::Atom("true".to_string()), None)),
        CorePattern::Var(name) => Ok((
            CoreExpr::Atom("true".to_string()),
            Some(PayloadBinding {
                name: name.clone(),
                operation: "body_json_identity",
            }),
        )),
        _ => Err(
            "error[native_ir.http_body_json_case]: expected Ok, Err, variable, or wildcard pattern"
                .to_string(),
        ),
    }
}

/// One lexical payload introduced around a result branch expression.
struct PayloadBinding {
    name: String,
    operation: &'static str,
}

/// Introduces one branch-local result or payload binding.
fn bind_payload(binding: Option<&PayloadBinding>, body: CoreExpr) -> CoreExpr {
    let Some(binding) = binding else {
        return body;
    };
    let value = if binding.operation == "body_json_identity" {
        CoreExpr::Var(BODY_JSON_TEMPORARY.to_string())
    } else {
        managed_call(
            binding.operation,
            vec![CoreExpr::Var(BODY_JSON_TEMPORARY.to_string())],
        )
    };
    CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var(binding.name.clone()),
            value,
        }],
        body: Box::new(body),
    }
}

/// Creates one compiler-private managed HTTP call.
fn managed_call(function: &str, args: Vec<CoreExpr>) -> CoreExpr {
    CoreExpr::RemoteCall {
        module: MANAGED_HTTP_MODULE.to_string(),
        function: function.to_string(),
        args,
    }
}

/// Builds the checked source type represented by the managed result layouts.
fn body_json_result_type() -> CoreType {
    CoreType::Apply {
        constructor: "Result".to_string(),
        args: vec![
            CoreType::Named("Json".to_string()),
            CoreType::Named("Error".to_string()),
        ],
    }
}
