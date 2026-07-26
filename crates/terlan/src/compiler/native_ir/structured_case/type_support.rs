//! Type recovery and validation shared by structured-pattern lowering.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CoreTupleTypeElem, CoreType};

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
    match expr {
        CoreExpr::Int(_) => Some(CoreType::Int),
        CoreExpr::Float(_) => Some(CoreType::Float),
        CoreExpr::Binary(_) => Some(CoreType::String),
        CoreExpr::Atom(value) if matches!(value.as_str(), "true" | "false") => Some(CoreType::Bool),
        CoreExpr::Atom(_) => Some(CoreType::Atom),
        CoreExpr::Var(name) => types.get(name).cloned(),
        CoreExpr::Call { function, args } => {
            functions.get(&(function.clone(), args.len())).cloned()
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
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        CoreExpr::Intrinsic(call) => Some(call.return_type.clone()),
        _ => None,
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
