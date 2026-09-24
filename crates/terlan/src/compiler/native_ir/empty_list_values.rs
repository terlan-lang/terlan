//! Per-use layout adaptation for the unique checked List[Never] value.

use std::collections::HashMap;

use super::{NativeConstructorLayouts, NativeType};
use crate::terlan_typeck::{CoreExpr, CoreLetBinding, CorePattern, CoreType};

#[cfg(test)]
#[path = "empty_list_values_test.rs"]
mod tests;

pub(super) fn is_bottom_list(ty: &CoreType) -> bool {
    matches!(ty, CoreType::List(element) if element.as_ref() == &CoreType::Never)
}

/// Applies a checked consumer layout after native lexical types are available.
/// Bare literals already use the boundary's expected type; other bottom values
/// reuse ordinary coercion so effectful producers still execute exactly once.
pub(super) fn at_boundary(
    value: &CoreExpr,
    expected: &CoreType,
    variables: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Option<CoreExpr> {
    if !matches!(expected, CoreType::List(_))
        || is_bottom_list(expected)
        || matches!(value, CoreExpr::List(items) if items.is_empty())
    {
        return None;
    }
    let bottom = CoreType::List(Box::new(CoreType::Never));
    let bottom_native = super::native_type(Some(&bottom), &bottom.contract_text())?;
    if super::infer_native_type_with_constructors(value, variables, functions, constructors)
        != Some(bottom_native)
    {
        return None;
    }
    let mut adapted = value.clone();
    coerce(&mut adapted, &bottom, expected);
    Some(adapted)
}

/// Refines only the uninhabited element type. A producer is evaluated once before
/// constructing the consumer's empty value; no concrete collection is retyped.
pub(super) fn coerce(expr: &mut CoreExpr, actual: &CoreType, expected: &CoreType) {
    if !is_bottom_list(actual) || !matches!(expected, CoreType::List(_)) || actual == expected {
        return;
    }
    let empty = CoreExpr::Cast {
        expr: Box::new(CoreExpr::List(Vec::new())),
        target_type: expected.clone(),
    };
    let source = std::mem::replace(expr, empty.clone());
    let is_pure_empty = matches!(&source, CoreExpr::Var(_))
        || matches!(&source, CoreExpr::List(items) if items.is_empty())
        || matches!(&source, CoreExpr::Cast { expr, .. } if matches!(expr.as_ref(), CoreExpr::List(items) if items.is_empty()));
    if !is_pure_empty {
        *expr = CoreExpr::Let {
            bindings: vec![CoreLetBinding {
                pattern: CorePattern::Var("$aot_empty_source".to_string()),
                value: source,
            }],
            body: Box::new(empty),
        };
    }
}
