use crate::terlan_typeck::{CoreExpr, CoreType};

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
        CoreExpr::FixedArray(items) => inferred_homogeneous_list_type(items),
        CoreExpr::Cast { target_type, .. } => Some(target_type.clone()),
        CoreExpr::Intrinsic(call) => Some(call.return_type.clone()),
        CoreExpr::Lam { params, body } => Some(CoreType::Arrow {
            params: vec![CoreType::Dynamic; params.len()],
            return_type: Box::new(inferred_core_expr_type(body).unwrap_or(CoreType::Dynamic)),
        }),
        _ => None,
    }
}

/// Infers a list type when every element has the same obvious CoreIR type.
///
/// Inputs:
/// - `items`: literal list or fixed-array elements.
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
