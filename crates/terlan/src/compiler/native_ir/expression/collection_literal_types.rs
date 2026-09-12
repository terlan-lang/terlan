//! Type recovery for homogeneous collection literals containing variables.

use crate::terlan_typeck::{CoreExpr, CoreType};

use super::NativeType;

pub(super) fn inferred_collection_literal_type(
    expr: &CoreExpr,
    mut infer: impl FnMut(&CoreExpr) -> Option<NativeType>,
    mut infer_managed: impl FnMut(&CoreExpr) -> Option<CoreType>,
) -> Option<CoreType> {
    infer_collection(expr, &mut infer, &mut infer_managed)
}

fn infer_collection(
    expr: &CoreExpr,
    infer: &mut dyn FnMut(&CoreExpr) -> Option<NativeType>,
    infer_managed: &mut dyn FnMut(&CoreExpr) -> Option<CoreType>,
) -> Option<CoreType> {
    let CoreExpr::List(items) = expr else {
        return None;
    };
    homogeneous_list_type(items, |item| {
        if matches!(item, CoreExpr::List(_)) {
            return infer_collection(item, infer, infer_managed);
        }
        infer(item).and_then(|ty| match ty {
            NativeType::Unit => Some(CoreType::Named("Unit".into())),
            NativeType::Int => Some(CoreType::Int),
            NativeType::Float => Some(CoreType::Float),
            NativeType::Bool => Some(CoreType::Bool),
            NativeType::Atom => Some(CoreType::Atom),
            NativeType::StringRef => Some(CoreType::String),
            NativeType::BytesRef => Some(CoreType::Named("Bytes".into())),
            NativeType::BinaryRef => Some(CoreType::Named("BitString".into())),
            NativeType::ManagedRef(_) => infer_managed(item),
        })
    })
}

/// Recover a homogeneous list from a concrete witness, regardless of its
/// position. An empty literal contributes shape, never an invented type.
pub(in crate::compiler::native_ir) fn homogeneous_list_type(
    items: &[CoreExpr],
    infer: impl FnMut(&CoreExpr) -> Option<CoreType>,
) -> Option<CoreType> {
    let types = items.iter().map(infer).collect::<Vec<_>>();
    let element = types.iter().find_map(Option::as_ref)?.clone();
    items
        .iter()
        .zip(types)
        .all(|(item, ty)| {
            ty.as_ref() == Some(&element)
                || (ty.is_none() && empty_list_shape_matches(item, &element))
        })
        .then(|| CoreType::List(Box::new(element)))
}

fn empty_list_shape_matches(expr: &CoreExpr, expected: &CoreType) -> bool {
    match (expr, expected) {
        (CoreExpr::List(items), CoreType::List(element)) => items
            .iter()
            .all(|item| empty_list_shape_matches(item, element)),
        _ => false,
    }
}
