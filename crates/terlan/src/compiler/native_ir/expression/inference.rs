use super::*;
use crate::terlan_typeck::{CoreIntrinsicId, CorePrimitiveIntrinsic, CoreType};

pub(in crate::compiler::native_ir) fn infer_native_type(
    expr: &CoreExpr,
    variables: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), NativeType>,
) -> Option<NativeType> {
    infer_native_type_impl(expr, variables, functions, None)
}

pub(in crate::compiler::native_ir) fn infer_native_type_with_constructors(
    expr: &CoreExpr,
    variables: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Option<NativeType> {
    infer_native_type_impl(expr, variables, functions, Some(constructors))
}

/// Infers a lowering type while preserving managed-projection diagnostics.
pub(in crate::compiler::native_ir) fn infer_native_type_for_lowering(
    expr: &CoreExpr,
    variables: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<Option<NativeType>, String> {
    match expr {
        CoreExpr::FieldAccess { base, field } => {
            let Some(base) =
                infer_native_type_for_lowering(base, variables, functions, constructors)?
            else {
                return Ok(None);
            };
            managed_field_projection(base, None, field, constructors).map(|(_, ty)| Some(ty))
        }
        CoreExpr::RecordAccess { base, name, field } => {
            let Some(base) =
                infer_native_type_for_lowering(base, variables, functions, constructors)?
            else {
                return Ok(None);
            };
            managed_field_projection(base, Some(name), field, constructors).map(|(_, ty)| Some(ty))
        }
        CoreExpr::RecordUpdate { base, name, .. } => {
            let Some(base) =
                infer_native_type_for_lowering(base, variables, functions, constructors)?
            else {
                return Ok(None);
            };
            record_update_result_type(name, base, constructors).map(Some)
        }
        _ => Ok(infer_native_type_with_constructors(
            expr,
            variables,
            functions,
            constructors,
        )),
    }
}

/// Shared recursive native-type inference implementation.
pub(super) fn infer_native_type_impl(
    expr: &CoreExpr,
    variables: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), NativeType>,
    constructors: Option<&NativeConstructorLayouts>,
) -> Option<NativeType> {
    if let Some(ty) = super::super::template_values::managed_template_operation_type(expr) {
        return Some(ty);
    }
    if let Some(ty) = super::super::http_values::managed_http_operation_type(expr) {
        return Some(ty);
    }
    match expr {
        CoreExpr::Atom(value) | CoreExpr::Var(value) if value == "Unit" => Some(NativeType::Unit),
        CoreExpr::Int(_) => Some(NativeType::Int),
        CoreExpr::Float(_) => Some(NativeType::Float),
        CoreExpr::Intrinsic(call)
            if matches!(
                call.id,
                CoreIntrinsicId::Primitive(
                    CorePrimitiveIntrinsic::ListConcat
                        | CorePrimitiveIntrinsic::ListSubtract
                        | CorePrimitiveIntrinsic::ListIterator
                        | CorePrimitiveIntrinsic::ListPush
                        | CorePrimitiveIntrinsic::ListClear
                )
            ) =>
        {
            call.args.first().and_then(|operand| {
                // These list operations preserve the receiver's concrete
                // element type. Their registry signature is necessarily
                // polymorphic (`List[Dynamic]`), but a continuation capture
                // must use the call-site specialization or it will reject the
                // typed list produced by the operation when the actor resumes.
                infer_native_type_impl(operand, variables, functions, constructors)
            })
        }
        CoreExpr::Intrinsic(call) => intrinsics::infer_intrinsic_type(call),
        CoreExpr::Binary(_) => Some(NativeType::StringRef),
        CoreExpr::Atom(value) | CoreExpr::Var(value)
            if matches!(value.as_str(), "true" | "false") =>
        {
            Some(NativeType::Bool)
        }
        CoreExpr::Atom(_) => Some(NativeType::Atom),
        CoreExpr::List(_) => {
            let ty = literal_collection_type(expr).or_else(|| {
                collection_literal_types::inferred_collection_literal_type(
                    expr,
                    |item| infer_native_type_impl(item, variables, functions, constructors),
                    |item| {
                        constructors.and_then(|layouts| {
                            constructor_result_core_type(item, layouts).or_else(|| {
                                infer_native_type_impl(item, variables, functions, constructors)
                                    .and_then(|native| {
                                        super::super::constructors::result_core_type_for_native(
                                            native, layouts,
                                        )
                                    })
                            })
                        })
                    },
                )
            })?;
            native_type(Some(&ty), &ty.contract_text())
        }
        CoreExpr::ListCons { tail, .. } => {
            let ty = infer_native_type_impl(tail, variables, functions, constructors)?;
            matches!(ty, NativeType::ManagedRef(_)).then_some(ty)
        }
        CoreExpr::Var(name) => variables.get(name).copied(),
        CoreExpr::Call { function, args }
            if matches!(
                function.as_str(),
                "std.core.Option.with_default" | "std.core.Result.with_default"
            ) && args.len() == 2 =>
        {
            // Both helpers return their second argument's generic value type.
            // Application-wide callable metadata cannot retain a concrete
            // NativeType for a polymorphic return, but the checked default at
            // each call site always supplies that specialization.
            infer_native_type_impl(&args[1], variables, functions, constructors)
        }
        CoreExpr::Call { function, args } => {
            functions.get(&(function.clone(), args.len())).copied()
        }
        CoreExpr::ConstructorCall { .. } => {
            constructors.and_then(|layouts| constructor_result_type(expr, layouts))
        }
        CoreExpr::RecordConstruct { .. } => {
            constructors.and_then(|layouts| record_construct_result_type(expr, layouts))
        }
        CoreExpr::RecordUpdate { base, name, .. } => {
            let base = infer_native_type_impl(base, variables, functions, constructors)?;
            record_update_result_type(name, base, constructors?).ok()
        }
        CoreExpr::FieldAccess { base, field } => {
            let base = infer_native_type_impl(base, variables, functions, constructors)?;
            managed_field_projection(base, None, field, constructors?)
                .ok()
                .map(|(_, ty)| ty)
        }
        CoreExpr::RecordAccess { base, name, field } => {
            let base = infer_native_type_impl(base, variables, functions, constructors)?;
            managed_field_projection(base, Some(name), field, constructors?)
                .ok()
                .map(|(_, ty)| ty)
        }
        CoreExpr::UnaryOp { operator, operand } => match operator.as_str() {
            "-" => match infer_native_type_impl(operand, variables, functions, constructors) {
                Some(ty @ (NativeType::Int | NativeType::Float)) => Some(ty),
                _ => None,
            },
            "not" | "!" => (infer_native_type_impl(operand, variables, functions, constructors)
                == Some(NativeType::Bool))
            .then_some(NativeType::Bool),
            _ => None,
        },
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            let left_type = infer_native_type_impl(left, variables, functions, constructors);
            let right_type = infer_native_type_impl(right, variables, functions, constructors);
            if matches!(operator.as_str(), "==" | "!=")
                && ((is_empty_list(left) && matches!(right_type, Some(NativeType::ManagedRef(_))))
                    || (is_empty_list(right)
                        && matches!(left_type, Some(NativeType::ManagedRef(_))))
                    || matches!(
                        (left_type, right_type),
                        (Some(NativeType::ManagedRef(_)), Some(NativeType::Atom))
                            | (Some(NativeType::Atom), Some(NativeType::ManagedRef(_)))
                    ))
            {
                return Some(NativeType::Bool);
            }
            let left = left_type?;
            let right = right_type?;
            match operator.as_str() {
                "+" | "-" | "*" | "/"
                    if matches!(left, NativeType::Int | NativeType::Float)
                        && matches!(right, NativeType::Int | NativeType::Float) =>
                {
                    Some(if left == NativeType::Float || right == NativeType::Float {
                        NativeType::Float
                    } else {
                        NativeType::Int
                    })
                }
                "+" if left == NativeType::StringRef && right == NativeType::StringRef => {
                    Some(NativeType::StringRef)
                }
                "div" | "rem" if left == NativeType::Int && right == NativeType::Int => {
                    Some(NativeType::Int)
                }
                "==" | "!="
                    if left == right
                        && (scalar_types::native_word_equality(left)
                            || managed_equality_semantic(left).is_some()) =>
                {
                    Some(NativeType::Bool)
                }
                "<" | "<=" | ">" | ">="
                    if matches!(left, NativeType::Int | NativeType::Float)
                        && matches!(right, NativeType::Int | NativeType::Float) =>
                {
                    Some(NativeType::Bool)
                }
                "and" | "or" if left == NativeType::Bool && right == NativeType::Bool => {
                    Some(NativeType::Bool)
                }
                _ => None,
            }
        }
        CoreExpr::Let { bindings, body } => {
            let mut locals = variables.clone();
            for binding in bindings {
                let CorePattern::Var(name) = &binding.pattern else {
                    return None;
                };
                let ty = infer_native_type_impl(&binding.value, &locals, functions, constructors)?;
                locals.insert(name.clone(), ty);
            }
            infer_native_type_impl(body, &locals, functions, constructors)
        }
        CoreExpr::If { clauses } => {
            let mut types = clauses.iter().map(|clause| {
                infer_native_type_impl(&clause.body, variables, functions, constructors)
            });
            let first = types.next()??;
            types.all(|ty| ty == Some(first)).then_some(first)
        }
        CoreExpr::Case { clauses, .. } => {
            let mut types = clauses.iter().map(|clause| {
                infer_native_type_impl(&clause.body, variables, functions, constructors)
            });
            let direct = types
                .next()
                .flatten()
                .filter(|first| types.all(|ty| ty == Some(*first)));
            direct.or_else(|| {
                let constructors = constructors?;
                let core_variables = core_types_from_native(variables, constructors);
                let core_functions = core_types_from_native(functions, constructors);
                let core_type = super::super::structured_case::core_expr_type(
                    expr,
                    &core_variables,
                    &core_functions,
                )?;
                native_type(Some(&core_type), &core_type.contract_text())
            })
        }
        CoreExpr::Cast { expr, target_type } => {
            if matches!(expr.as_ref(), CoreExpr::Binary(_))
                && matches!(target_type, CoreType::Binary | CoreType::String)
            {
                return native_type(Some(target_type), &target_type.contract_text());
            }
            if matches!(
                expr.as_ref(),
                CoreExpr::List(_) | CoreExpr::Tuple(_) | CoreExpr::Map(_)
            ) {
                return native_type(Some(target_type), &target_type.contract_text());
            }
            if matches!(expr.as_ref(), CoreExpr::ConstructorCall { .. }) {
                return native_type(Some(target_type), &target_type.contract_text());
            }
            let source = infer_native_type_impl(expr, variables, functions, constructors)?;
            let target = native_type(Some(target_type), &target_type.contract_text())?;
            (source == target).then_some(target)
        }
        _ => None,
    }
}

/// Recovers checked CoreIR types for native values whose source identities are
/// retained by scalar kinds or managed constructor layouts.
pub(in crate::compiler::native_ir) fn core_types_from_native<K>(
    values: &HashMap<K, NativeType>,
    constructors: &NativeConstructorLayouts,
) -> HashMap<K, CoreType>
where
    K: Clone + Eq + std::hash::Hash,
{
    values
        .iter()
        .filter_map(|(name, ty)| {
            core_type_from_native(*ty, constructors).map(|core| (name.clone(), core))
        })
        .collect()
}

/// Maps one compact native kind back to the checked type needed for lexical
/// case-pattern inference.
fn core_type_from_native(
    ty: NativeType,
    constructors: &NativeConstructorLayouts,
) -> Option<CoreType> {
    match ty {
        NativeType::Unit => Some(CoreType::Named("Unit".into())),
        NativeType::Int => Some(CoreType::Int),
        NativeType::Float => Some(CoreType::Float),
        NativeType::Bool => Some(CoreType::Bool),
        NativeType::Atom => Some(CoreType::Atom),
        NativeType::StringRef => Some(CoreType::String),
        NativeType::BytesRef => Some(CoreType::Named("Bytes".into())),
        NativeType::BinaryRef => Some(CoreType::Named("BitString".into())),
        NativeType::ManagedRef(_) => {
            super::super::constructors::result_core_type_for_native(ty, constructors)
        }
    }
}
