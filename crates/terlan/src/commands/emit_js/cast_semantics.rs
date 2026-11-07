use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CorePattern, CoreTupleTypeElem, CoreType};

/// Returns whether a CoreIR cast can lower as a JavaScript identity expression.
///
/// Inputs:
/// - `expr`: CoreIR source expression being cast.
/// - `target_type`: CoreIR target type preserved from Terlan `as` syntax.
///
/// Output:
/// - `true` when the JS backend can prove the source already has the target
///   shape and no runtime conversion call is required.
/// - `false` when the source type is unknown to this backend layer or the cast
///   would require an explicit conversion implementation.
///
/// Transformation:
/// - Infers only obvious source types from CoreIR literals and typed CoreIR
///   nodes, then applies the small assignment-compatibility relation that JS
///   emission may erase safely. This deliberately refuses trait-backed
///   conversions until they lower as explicit conversion calls.
pub(super) fn cast_can_lower_as_js_identity(expr: &CoreExpr, target_type: &CoreType) -> bool {
    // Contextual generic specialization annotates [] with its checked element
    // type. There are no values to convert; this is not a widening of an
    // arbitrary List[Dynamic] value or a cast to a target-specific wrapper.
    if matches!(expr, CoreExpr::List(items) | CoreExpr::Array(items) if items.is_empty())
        && matches!(target_type, CoreType::List(_))
    {
        return true;
    }
    inferred_core_expr_type(expr)
        .as_ref()
        .is_some_and(|source_type| core_type_is_js_identity_assignable(source_type, target_type))
}

/// Infers the obvious CoreIR type of an expression for JS cast classification.
///
/// Inputs:
/// - `expr`: CoreIR expression selected by the JS backend.
///
/// Output:
/// - `Some(CoreType)` for literals and typed CoreIR boundaries whose source
///   type is explicit in the node.
/// - `None` when the source depends on lexical variables, overload resolution,
///   imports, or richer module context not carried by the current JS emitter.
///
/// Transformation:
/// - Reads type information already present in CoreIR without reaching back
///   into syntax, HIR, or typechecker internals.
fn inferred_core_expr_type(expr: &CoreExpr) -> Option<CoreType> {
    match expr {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Atom(value) if value == "true" || value == "false" => Some(CoreType::Bool),
        CoreExpr::Atom(value) => Some(CoreType::AtomLiteral(value.clone())),
        CoreExpr::List(items) => inferred_homogeneous_list_type(items),
        CoreExpr::Array(items) => inferred_homogeneous_list_type(items),
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        CoreExpr::Intrinsic(call) => Some(call.return_type.clone()),
        CoreExpr::Lam {
            params,
            parameter_types,
            body,
        } => Some(CoreType::Arrow {
            params: (0..params.len())
                .map(|index| {
                    parameter_types
                        .get(index)
                        .and_then(Option::as_ref)
                        .cloned()
                        .unwrap_or(CoreType::Dynamic)
                })
                .collect(),
            return_type: Box::new(inferred_core_expr_type(body).unwrap_or(CoreType::Dynamic)),
        }),
        _ => None,
    }
}

/// Infers a list type when every element has the same obvious CoreIR type.
///
/// Inputs:
/// - `items`: literal list or array elements.
///
/// Output:
/// - `Some(CoreType::List(_))` when all elements infer to one type, or an
///   empty dynamic list for empty literals.
/// - `None` when any element is unknown or element types differ.
///
/// Transformation:
/// - Converts only homogeneous literal collections into JS cast evidence so
///   list/array wrappers are not silently treated as arbitrary target shapes.
fn inferred_homogeneous_list_type(items: &[CoreExpr]) -> Option<CoreType> {
    let mut item_types = items.iter().map(inferred_core_expr_type);
    let first = match item_types.next() {
        Some(Some(first)) => first,
        Some(None) => return None,
        None => return Some(CoreType::List(Box::new(CoreType::Dynamic))),
    };
    if item_types.all(|item_type| item_type.as_ref() == Some(&first)) {
        Some(CoreType::List(Box::new(first)))
    } else {
        None
    }
}

/// Checks whether a source type can be erased to a target type in JS output.
///
/// Inputs:
/// - `source_type`: obvious source type inferred from CoreIR.
/// - `target_type`: requested cast target type.
///
/// Output:
/// - `true` for exact type matches and safe primitive widenings.
/// - `false` for conversions, unknown dynamic views, and target-specific
///   wrappers that need explicit conversion lowering.
///
/// Transformation:
/// - Implements only the JS backend's identity-erasure relation, not the full
///   Terlan typechecker subtype relation.
fn core_type_is_js_identity_assignable(source_type: &CoreType, target_type: &CoreType) -> bool {
    source_type == target_type
        || matches!(
            (source_type, target_type),
            (CoreType::Int | CoreType::Float, CoreType::Number)
                | (CoreType::AtomLiteral(_), CoreType::Atom)
                | (_, CoreType::Term)
        )
}

#[cfg(test)]
#[path = "cast_semantics_test.rs"]
mod tests;

/// Specialization can annotate call results and lists with identity casts.
/// Erase only casts proved assignable by the linked, unique function signatures.
pub(super) fn erase_linked_identity_casts(core: &mut crate::terlan_typeck::CoreModule) {
    let returns = core
        .functions
        .iter()
        .filter_map(|function| {
            function
                .core_return_type
                .clone()
                .map(|ty| ((function.name.clone(), function.arity), ty))
        })
        .collect::<HashMap<_, _>>();
    for function in &mut core.functions {
        for clause in &mut function.clauses {
            let mut environment = HashMap::new();
            for (index, parameter) in function.params.iter().enumerate() {
                let Some(parameter_type) = &parameter.core_ty else {
                    continue;
                };
                if let Some(pattern) = clause.core_patterns.get(index).and_then(Option::as_ref) {
                    bind_pattern_type(pattern, parameter_type, &mut environment);
                } else {
                    environment.insert(parameter.name.clone(), parameter_type.clone());
                }
            }
            for summary in clause
                .guard
                .iter_mut()
                .chain(std::iter::once(&mut clause.body))
            {
                if let Some(expr) = &mut summary.core_expr {
                    erase_expr_identity_casts(expr, &returns, &environment);
                }
            }
        }
    }
}

/// Erases checked identity casts while retaining the lexical types in scope.
fn erase_expr_identity_casts(
    expr: &mut CoreExpr,
    returns: &HashMap<(String, usize), CoreType>,
    environment: &HashMap<String, CoreType>,
) -> Option<CoreType> {
    match expr {
        CoreExpr::Let { bindings, body } => {
            let mut nested = environment.clone();
            for binding in bindings {
                if let Some(value_type) =
                    erase_expr_identity_casts(&mut binding.value, returns, &nested)
                {
                    bind_pattern_type(&binding.pattern, &value_type, &mut nested);
                }
            }
            erase_expr_identity_casts(body, returns, &nested)
        }
        CoreExpr::Case { scrutinee, clauses } => {
            let scrutinee_type = erase_expr_identity_casts(scrutinee, returns, environment);
            let branch_types = clauses
                .iter_mut()
                .map(|clause| {
                    let mut nested = environment.clone();
                    if let Some(scrutinee_type) = &scrutinee_type {
                        bind_pattern_type(&clause.pattern, scrutinee_type, &mut nested);
                    }
                    if let Some(guard) = &mut clause.guard {
                        erase_expr_identity_casts(guard, returns, &nested);
                    }
                    erase_expr_identity_casts(&mut clause.body, returns, &nested)
                })
                .collect::<Vec<_>>();
            common_type(&branch_types)
        }
        CoreExpr::Lam {
            params,
            parameter_types,
            body,
        } => {
            let mut nested = environment.clone();
            for (index, pattern) in params.iter().enumerate() {
                if let Some(parameter_type) = parameter_types.get(index).and_then(Option::as_ref) {
                    bind_pattern_type(pattern, parameter_type, &mut nested);
                }
            }
            let return_type = erase_expr_identity_casts(body, returns, &nested)?;
            Some(CoreType::Arrow {
                params: parameter_types
                    .iter()
                    .map(|parameter| parameter.clone().unwrap_or(CoreType::Dynamic))
                    .collect(),
                return_type: Box::new(return_type),
            })
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            let mut nested = environment.clone();
            for generator in generators {
                let source_type =
                    erase_expr_identity_casts(&mut generator.source, returns, &nested);
                if let Some(CoreType::List(item_type)) = source_type {
                    bind_pattern_type(&generator.pattern, &item_type, &mut nested);
                }
            }
            for guard in guards {
                erase_expr_identity_casts(guard, returns, &nested);
            }
            erase_expr_identity_casts(expr, returns, &nested)
                .map(|item_type| CoreType::List(Box::new(item_type)))
        }
        _ => {
            crate::terlan_typeck::visit_core_expr_children_mut(expr, &mut |child| {
                erase_expr_identity_casts(child, returns, environment);
            });
            let replacement = match expr {
                CoreExpr::Cast {
                    expr: value,
                    target_type,
                } if linked_expr_type(value, returns, environment)
                    .as_ref()
                    .is_some_and(|source| {
                        core_type_is_js_identity_assignable(source, target_type)
                    }) =>
                {
                    Some(*std::mem::replace(value, Box::new(CoreExpr::Int(0))))
                }
                _ => None,
            };
            if let Some(replacement) = replacement {
                *expr = replacement;
            }
            linked_expr_type(expr, returns, environment)
        }
    }
}

fn linked_expr_type(
    expr: &CoreExpr,
    returns: &HashMap<(String, usize), CoreType>,
    environment: &HashMap<String, CoreType>,
) -> Option<CoreType> {
    match expr {
        CoreExpr::Var(name) => environment.get(name).cloned(),
        CoreExpr::Call { function, args, .. } => {
            returns.get(&(function.clone(), args.len())).cloned()
        }
        CoreExpr::List(items) | CoreExpr::Array(items) if !items.is_empty() => {
            let first = linked_expr_type(&items[0], returns, environment)?;
            items[1..]
                .iter()
                .all(|item| linked_expr_type(item, returns, environment).as_ref() == Some(&first))
                .then(|| CoreType::List(Box::new(first)))
        }
        _ => inferred_core_expr_type(expr),
    }
}

/// Binds lexical names to the checked structural type of their pattern.
fn bind_pattern_type(
    pattern: &CorePattern,
    ty: &CoreType,
    environment: &mut HashMap<String, CoreType>,
) {
    match pattern {
        CorePattern::Var(name) => {
            environment.insert(name.clone(), ty.clone());
        }
        CorePattern::Alias { alias, pattern } => {
            environment.insert(alias.clone(), ty.clone());
            bind_pattern_type(pattern, ty, environment);
        }
        CorePattern::Tuple(patterns) => {
            if let CoreType::Tuple(elements) = ty {
                for (pattern, element) in patterns.iter().zip(elements) {
                    let element_type = match element {
                        CoreTupleTypeElem::Type(element_type)
                        | CoreTupleTypeElem::Field {
                            ty: element_type, ..
                        } => element_type,
                    };
                    bind_pattern_type(pattern, element_type, environment);
                }
            }
        }
        CorePattern::List(patterns) => {
            if let CoreType::List(item_type) = ty {
                for pattern in patterns {
                    bind_pattern_type(pattern, item_type, environment);
                }
            }
        }
        CorePattern::ListCons { head, tail } => {
            if let CoreType::List(item_type) = ty {
                bind_pattern_type(head, item_type, environment);
                bind_pattern_type(tail, ty, environment);
            }
        }
        CorePattern::Constructor { name, args, .. } => {
            if let (Some(payload), Some(payload_type)) =
                (args.first(), constructor_pattern_payload(ty, name))
            {
                bind_pattern_type(payload, payload_type, environment);
            }
        }
        _ => {}
    }
}

fn constructor_pattern_payload<'a>(ty: &'a CoreType, name: &str) -> Option<&'a CoreType> {
    let constructor = name.rsplit('.').next()?;
    match ty {
        CoreType::Apply {
            constructor: applied,
            args,
        } if applied.rsplit('.').next() == Some("Result") && args.len() == 2 => match constructor {
            "Ok" => args.first(),
            "Err" => args.get(1),
            _ => None,
        },
        CoreType::Apply {
            constructor: applied,
            args,
        } if applied.rsplit('.').next() == Some("Option") && args.len() == 1 => {
            (constructor == "Some").then(|| &args[0])
        }
        _ => None,
    }
}

fn common_type(types: &[Option<CoreType>]) -> Option<CoreType> {
    let first = types.first()?.as_ref()?;
    types
        .iter()
        .all(|ty| ty.as_ref() == Some(first))
        .then(|| first.clone())
}
