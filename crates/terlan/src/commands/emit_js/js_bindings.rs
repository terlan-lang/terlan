//! JavaScript host operations retain their pinned TypeScript names and call shape.

use std::collections::BTreeMap;
use std::fmt;

use crate::terlan_typeck::{visit_core_expr_mut, CoreExpr, CoreIntrinsicId, CoreModule};

/// A checked host call has no matching binding or has an incompatible arity.
#[derive(Debug)]
pub(super) struct BindingError(String);

impl fmt::Display for BindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "error[js.binding]: {}", self.0)
    }
}

impl std::error::Error for BindingError {}

/// Native-only providers are supplied by the selected JavaScript host.
pub(super) fn is_binding_module(module: &str) -> bool {
    crate::support::js_binding_registry::registry()
        .is_ok_and(|registry| registry.modules.contains_key(module))
}

/// Materializes foreign declarations as typed host operations before source linking.
pub(super) fn materialize(core: &mut CoreModule) {
    use crate::terlan_typeck::{
        CoreEffectSet, CoreExprSummary, CoreFunctionClause, CoreIntrinsicCall, CorePattern,
        CoreProofCoverage,
    };
    if !core.module.starts_with("std.js.") && !is_binding_module(&core.module) {
        return;
    }
    for function in &mut core.functions {
        let native = function.clauses.is_empty()
            || matches!(function.clauses.as_slice(),
            [clause] if matches!(clause.body.core_expr.as_ref(), Some(CoreExpr::Var(name) | CoreExpr::Atom(name)) if name == "native"));
        if !native {
            continue;
        }
        let Some(parameter_types) = function
            .params
            .iter()
            .map(|parameter| parameter.core_ty.clone())
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        let Some(return_type) = function.core_return_type.clone() else {
            continue;
        };
        let operation = format!("{}.{}", core.module, function.name);
        let names = function
            .params
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect::<Vec<_>>();
        function.native_operation = Some(operation.clone());
        function.clauses = vec![CoreFunctionClause {
            patterns: names.clone(),
            core_patterns: names
                .iter()
                .map(|name| Some(CorePattern::Var(name.clone())))
                .collect(),
            pattern_proof_coverage: vec![CoreProofCoverage::LeanCovered; names.len()],
            pattern_checked_preservation_evidence: vec![None; names.len()],
            guard: None,
            body: CoreExprSummary {
                kind: "NativeOperation".into(),
                core_expr: Some(CoreExpr::Intrinsic(CoreIntrinsicCall {
                    id: CoreIntrinsicId::NativeOperation {
                        operation,
                        parameter_types,
                    },
                    args: names.into_iter().map(CoreExpr::Var).collect(),
                    return_type,
                    effects: CoreEffectSet {
                        effects: vec!["js-host".into()],
                    },
                    span: crate::terlan_syntax::span::Span::new(0, 0),
                })),
                checked_preservation_evidence: None,
                proof_coverage: CoreProofCoverage::ProofModelRequired,
                text: None,
                remote: None,
                operator: None,
                arity: function.arity,
                children: Vec::new(),
            },
        }];
    }
}

/// Lowers checked foreign calls and emits only the host adapters they require.
pub(super) fn lower(core: &mut CoreModule) -> Result<String, BindingError> {
    crate::support::js_binding_registry::registry().map_err(BindingError)?;
    let mut required = BTreeMap::new();
    let mut failure = None;
    for function in &mut core.functions {
        for clause in &mut function.clauses {
            for summary in clause
                .guard
                .iter_mut()
                .chain(std::iter::once(&mut clause.body))
            {
                let Some(expression) = &mut summary.core_expr else {
                    continue;
                };
                visit_core_expr_mut(expression, &mut |expression| {
                    let (operation, arity) = match expression {
                        CoreExpr::Call { function, args, .. } if is_host_operation(function) => {
                            (function.clone(), args.len())
                        }
                        CoreExpr::RemoteCall {
                            module,
                            function,
                            args,
                            ..
                        } if is_binding_module(module) => {
                            (format!("{module}.{function}"), args.len())
                        }
                        CoreExpr::Intrinsic(call) => {
                            let CoreIntrinsicId::NativeOperation { operation, .. } = &call.id
                            else {
                                return;
                            };
                            if !is_host_operation(operation) {
                                return;
                            }
                            (operation.clone(), call.args.len())
                        }
                        _ => return,
                    };
                    match adapter(&operation, arity) {
                        Ok(source) => {
                            let name = format!("$js${}", operation.replace('.', "$"));
                            required.insert(name.clone(), source);
                            match expression {
                                CoreExpr::Call { function, .. } => *function = name,
                                CoreExpr::RemoteCall {
                                    args, type_args, ..
                                } => {
                                    *expression = CoreExpr::Call {
                                        function: name,
                                        args: std::mem::take(args),
                                        type_args: std::mem::take(type_args),
                                    };
                                }
                                CoreExpr::Intrinsic(call) => {
                                    *expression = CoreExpr::Call {
                                        function: name,
                                        args: std::mem::take(&mut call.args),
                                        type_args: Vec::new(),
                                    };
                                }
                                _ => unreachable!(),
                            }
                        }
                        Err(error) => failure = Some(error),
                    }
                });
            }
        }
    }
    if let Some(error) = failure {
        return Err(error);
    }
    Ok(required
        .into_iter()
        .map(|(name, (arity, expression))| {
            let parameters = (0..arity)
                .map(|index| format!("a{index}"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("function {name}({parameters}) {{ return {expression}; }}\n")
        })
        .collect())
}

fn is_host_operation(operation: &str) -> bool {
    operation.starts_with("std.js.")
        || operation
            .rsplit_once('.')
            .is_some_and(|(module, _)| is_binding_module(module))
}

fn adapter(operation: &str, arity: usize) -> Result<(usize, String), BindingError> {
    let bridge = match operation {
        "std.js.String.from_core"
        | "std.js.String.to_core"
        | "std.js.String.to_string"
        | "std.js.Number.from_float"
        | "std.js.Number.to_float" => Some((1, "a0")),
        "std.js.Number.to_string" => Some((1, "String(a0)")),
        "std.js.Array.from_list" | "std.js.Array.to_list" => Some((1, "a0.slice()")),
        "std.js.Array.length" => Some((1, "a0.length")),
        "std.js.Array.get_at" => Some((2, "a0[a1]")),
        "std.js.Array.iterator" => Some((1, "a0.values()")),
        "std.js.Promise.from_task" | "std.js.Promise.to_task" => Some((1, "a0")),
        _ => None,
    };
    if let Some((expected, expression)) = bridge {
        return checked_arity(operation, arity, expected).map(|()| (arity, expression.into()));
    }
    let (module, name) = operation.rsplit_once('.').unwrap_or(("", ""));
    let registry = crate::support::js_binding_registry::registry().map_err(BindingError)?;
    let member = registry
        .modules
        .get(module)
        .into_iter()
        .flatten()
        .find(|member| member.name == name && member.arity == arity)
        .ok_or_else(|| BindingError(format!("unsupported host operation {operation}/{arity}")))?;
    member_expression(member).map(|expression| (arity, expression))
}

fn member_expression(
    member: &crate::support::js_binding_registry::Member,
) -> Result<String, BindingError> {
    let key = serde_json::to_string(&member.js_name).expect("serializable property name");
    let result = match member.kind.as_str() {
        "mixin" => {
            let fields = member
                .js_names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    format!(
                        "{}: a{}",
                        serde_json::to_string(name).expect("serializable member name"),
                        index + 1
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("Object.assign(a0, {{{fields}}})")
        }
        "property" => format!("a0[{key}]"),
        "setter" => {
            let value = parameter_argument(&member.parameters[0], "a1");
            format!("(a0[{key}] = {value})")
        }
        "record" => {
            let fields = member
                .js_names
                .iter()
                .zip(&member.parameters)
                .enumerate()
                .map(|(index, (name, parameter))| {
                    let key = serde_json::to_string(name).expect("serializable record key");
                    let value = parameter_argument(parameter, &format!("a{index}"));
                    // Computed keys create an own property even for __proto__.
                    format!("[{key}]: {value}")
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("({{{fields}}})")
        }
        "method" => {
            let arguments = member
                .parameters
                .iter()
                .enumerate()
                .map(|(index, parameter)| {
                    let argument = format!("a{}", index + 1);
                    let value = parameter_argument(parameter, &argument);
                    if parameter.rest {
                        format!("...{value}")
                    } else {
                        value
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("a0[{key}]({arguments})")
        }
        _ => {
            return Err(BindingError(format!(
                "unknown host binding kind {}",
                member.kind
            )))
        }
    };
    let result = if member.return_type.starts_with("Option[")
        || member.return_type.starts_with("std.core.Option.Option[")
    {
        let missing = if member.undefined_only {
            "value === undefined"
        } else {
            "value == null"
        };
        format!("((value) => {missing} ? 'none' : ['some', value])({result})")
    } else if matches!(member.return_type.as_str(), "Unit" | "std.core.Unit.Unit") {
        format!("({result}, 'unit')")
    } else {
        result
    };
    Ok(result)
}

fn parameter_argument(
    parameter: &crate::support::js_binding_registry::Parameter,
    argument: &str,
) -> String {
    let missing = if parameter.undefined_only {
        "undefined"
    } else {
        "null"
    };
    if parameter.optional {
        let inner = decode_nullable_argument(&format!("{argument}[1]"), &parameter.ty, missing);
        format!("({argument} === 'none' ? undefined : {inner})")
    } else {
        decode_nullable_argument(argument, &parameter.ty, missing)
    }
}

fn decode_nullable_argument(argument: &str, ty: &str, missing: &str) -> String {
    let inner = ty
        .strip_prefix("Option[")
        .or_else(|| ty.strip_prefix("std.core.Option.Option["))
        .and_then(|inner| inner.strip_suffix(']'));
    if let Some(inner) = inner {
        let present = decode_nullable_argument(&format!("{argument}[1]"), inner, "null");
        format!("({argument} === 'none' ? {missing} : {present})")
    } else {
        argument.into()
    }
}

fn checked_arity(operation: &str, actual: usize, expected: usize) -> Result<(), BindingError> {
    if actual == expected {
        Ok(())
    } else {
        Err(BindingError(format!(
            "{operation} expects {expected} arguments, got {actual}"
        )))
    }
}

#[cfg(test)]
#[path = "js_bindings_test.rs"]
mod tests;
