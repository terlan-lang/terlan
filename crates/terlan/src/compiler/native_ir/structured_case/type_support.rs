//! Type recovery and validation shared by structured-pattern lowering.

use std::collections::HashMap;

use crate::terlan_typeck::{
    CoreExpr, CoreIntrinsicId, CorePattern, CorePrimitiveIntrinsic, CoreTupleTypeElem, CoreType,
};

use super::super::{native_type, NativeExpr, NativeType};

pub(super) fn native_core_type(ty: &CoreType) -> Result<NativeType, String> {
    native_type(Some(ty), &ty.contract_text()).ok_or_else(|| {
        format!(
            "error[native_ir.structured_pattern_type]: unsupported `{}`",
            ty.contract_text()
        )
    })
}

pub(super) fn core_expr_type(
    expr: &CoreExpr,
    types: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
) -> Option<CoreType> {
    let inferred = match expr {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(_) => Some(CoreType::Atom),
        CoreExpr::Var(name) => types.get(name).cloned(),
        CoreExpr::Call { function, args } => {
            functions.get(&(function.clone(), args.len())).cloned()
        }
        CoreExpr::FunctionCall { callee, args } => {
            let CoreType::Arrow {
                params,
                return_type,
            } = core_expr_type(callee, types, functions)?
            else {
                return None;
            };
            (params.len() == args.len()).then(|| return_type.as_ref().clone())
        }
        CoreExpr::Tuple(items) => items
            .iter()
            .map(|item| core_expr_type(item, types, functions).map(CoreTupleTypeElem::Type))
            .collect::<Option<Vec<_>>>()
            .map(CoreType::Tuple),
        CoreExpr::List(items) if !items.is_empty() => {
            let first = core_expr_type(&items[0], types, functions)?;
            items[1..]
                .iter()
                .all(|item| core_expr_type(item, types, functions) == Some(first.clone()))
                .then(|| CoreType::List(Box::new(first)))
        }
        CoreExpr::FieldAccess { base, field } | CoreExpr::RecordAccess { base, field, .. } => {
            let base = core_expr_type(base, types, functions)?;
            let CoreType::Struct { fields, .. } = base else {
                return None;
            };
            fields
                .into_iter()
                .find(|candidate| candidate.name == *field)
                .map(|candidate| candidate.ty)
        }
        CoreExpr::Index { base, index } => {
            let CoreExpr::Int(index) = index.as_ref() else {
                return None;
            };
            let index = usize::try_from(*index).ok()?;
            match core_expr_type(base, types, functions)? {
                CoreType::Tuple(elements) => elements.get(index).map(tuple_element_type).cloned(),
                CoreType::List(element) => Some(*element),
                CoreType::Apply { constructor, args }
                    if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
                {
                    args.into_iter().next()
                }
                _ => None,
            }
        }
        CoreExpr::Cast { expr, target_type }
            if matches!(target_type, CoreType::Dynamic)
                || matches!(target_type, CoreType::Named(name) if name == "Dynamic") =>
        {
            core_expr_type(expr, types, functions)
        }
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
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
            call.args
                .first()
                .and_then(|operand| core_expr_type(operand, types, functions))
        }
        CoreExpr::Intrinsic(call) => Some(call.return_type.clone()),
        CoreExpr::Let { bindings, body } => {
            let mut lexical = types.clone();
            for binding in bindings {
                let CorePattern::Var(name) = &binding.pattern else {
                    return None;
                };
                if let Some(ty) = core_expr_type(&binding.value, &lexical, functions) {
                    lexical.insert(name.clone(), ty);
                }
            }
            core_expr_type(body, &lexical, functions)
        }
        CoreExpr::If { clauses } => {
            control_result_type(clauses.iter().map(|clause| &clause.body), types, functions)
        }
        CoreExpr::Case { clauses, .. } => {
            control_result_type(clauses.iter().map(|clause| &clause.body), types, functions)
        }
        _ => None,
    };
    inferred.map(transparent_message_payload)
}

/// Recovers one checked control result while permitting the atom-form nullary
/// `None` arm to inherit the concrete managed `Option` representation from a
/// sibling. Type checking has already established branch compatibility; this
/// step restores the erased structural type needed by NativeIR allocation.
fn control_result_type<'a>(
    expressions: impl Iterator<Item = &'a CoreExpr>,
    types: &HashMap<String, CoreType>,
    functions: &HashMap<(String, usize), CoreType>,
) -> Option<CoreType> {
    let mut result = None;
    let mut inherited_none = false;
    for expression in expressions {
        if is_nullary_none(expression) {
            inherited_none = true;
            continue;
        }
        match core_expr_type(expression, types, functions) {
            Some(found) => match &result {
                Some(expected) if expected != &found => return None,
                None => result = Some(found),
                _ => {}
            },
            None => return None,
        }
    }
    let result = result?;
    if inherited_none && !option_like_type(&result) {
        return None;
    }
    Some(result)
}

fn is_nullary_none(expr: &CoreExpr) -> bool {
    match expr {
        CoreExpr::Atom(name) | CoreExpr::Var(name) => name.eq_ignore_ascii_case("none"),
        CoreExpr::ConstructorCall {
            constructor, args, ..
        } => args.is_empty() && constructor.rsplit('.').next() == Some("None"),
        CoreExpr::Cast { expr, .. } => is_nullary_none(expr),
        CoreExpr::Let { body, .. } => is_nullary_none(body),
        CoreExpr::If { clauses } => {
            !clauses.is_empty() && clauses.iter().all(|clause| is_nullary_none(&clause.body))
        }
        CoreExpr::Case { clauses, .. } => {
            !clauses.is_empty() && clauses.iter().all(|clause| is_nullary_none(&clause.body))
        }
        _ => false,
    }
}

fn option_like_type(ty: &CoreType) -> bool {
    match ty {
        CoreType::Apply { constructor, args } => {
            constructor.rsplit('.').next() == Some("Option") && args.len() == 1
        }
        CoreType::Union(variants) => variants
            .iter()
            .any(|variant| matches!(variant, CoreType::AtomLiteral(name) if name == "none")),
        _ => false,
    }
}

fn transparent_message_payload(ty: CoreType) -> CoreType {
    match ty {
        CoreType::Apply {
            constructor,
            mut args,
        } if constructor.rsplit('.').next() == Some("Message") && args.len() == 1 => args.remove(0),
        other => other,
    }
}

pub(super) fn tuple_element_type(element: &CoreTupleTypeElem) -> &CoreType {
    match element {
        CoreTupleTypeElem::Type(ty) | CoreTupleTypeElem::Field { ty, .. } => ty,
    }
}

pub(super) fn list_element_type(core_type: Option<&CoreType>) -> Result<&CoreType, String> {
    match core_type {
        Some(CoreType::List(element)) => Ok(element),
        Some(CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("List") && args.len() == 1 =>
        {
            Ok(&args[0])
        }
        _ => Err("error[native_ir.list_pattern_type]: concrete List type is unavailable".into()),
    }
}

pub(super) fn option_element_type(core_type: Option<&CoreType>) -> Option<&CoreType> {
    match core_type {
        Some(CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("Option") && args.len() == 1 =>
        {
            Some(&args[0])
        }
        Some(CoreType::Union(variants))
            if variants.iter().any(
                |variant| matches!(variant, CoreType::AtomLiteral(atom) if atom == "none"),
            ) =>
        {
            variants.iter().find_map(|variant| {
                let CoreType::Tuple(elements) = variant else {
                    return None;
                };
                let [tag, value] = elements.as_slice() else {
                    return None;
                };
                matches!(tuple_element_type(tag), CoreType::AtomLiteral(atom) if atom == "some")
                    .then(|| tuple_element_type(value))
            })
        }
        _ => None,
    }
}

pub(super) fn map_types(core_type: Option<&CoreType>) -> Result<(&CoreType, &CoreType), String> {
    match core_type {
        Some(CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("Map") && args.len() == 2 =>
        {
            Ok((&args[0], &args[1]))
        }
        _ => Err("error[native_ir.map_pattern_type]: concrete Map type is unavailable".into()),
    }
}

pub(super) fn struct_field_type<'a>(
    core_type: Option<&'a CoreType>,
    name: &str,
) -> Option<&'a CoreType> {
    let Some(CoreType::Struct { fields, .. }) = core_type else {
        return None;
    };
    fields
        .iter()
        .find(|field| field.name == name)
        .map(|field| &field.ty)
}

pub(super) fn map_key(key: &str, ty: &CoreType) -> Result<NativeExpr, String> {
    match ty {
        CoreType::String => {
            let encoded = crate::runtime::native_image::managed::encode_string_literal(key)
                .map_err(|error| format!("error[native_ir.map_pattern_key]: {error}"))?;
            Ok(NativeExpr::StringLiteral {
                encoded: encoded.into(),
            })
        }
        CoreType::Int => key
            .parse::<i64>()
            .map(NativeExpr::Int)
            .map_err(|error| format!("error[native_ir.map_pattern_key]: {error}")),
        _ => Err(format!(
            "error[native_ir.map_pattern_key]: unsupported key type `{}`",
            ty.contract_text()
        )),
    }
}
