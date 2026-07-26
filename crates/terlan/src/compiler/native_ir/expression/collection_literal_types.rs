//! Type recovery for homogeneous collection literals containing variables.

use crate::terlan_typeck::{CoreExpr, CoreType};

use super::NativeType;

pub(super) fn inferred_collection_literal_type(
    expr: &CoreExpr,
    mut infer: impl FnMut(&CoreExpr) -> Option<NativeType>,
    mut infer_managed: impl FnMut(&CoreExpr) -> Option<CoreType>,
) -> Option<CoreType> {
    let CoreExpr::List(items) = expr else {
        return None;
    };
    let mut types = items.iter().map(|item| {
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
    });
    let element = types.next()??;
    types
        .all(|ty| ty.as_ref() == Some(&element))
        .then(|| CoreType::List(Box::new(element)))
}
