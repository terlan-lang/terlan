//! Direct managed layouts for simple source constructor variants.

use crate::terlan_typeck::{CoreConstructorDecl, CoreExpr, CoreTupleTypeElem, CoreType};

/// Identifies source constructors that only forward their arguments into one
/// atom-tagged variant and therefore need no executable adapter.
///
/// Fixed layouts must own the entire union. Admitting both nullary atoms and
/// their tuple-payload siblings keeps one complete discriminant table while
/// arbitrary executable constructor bodies continue through ordinary calls.
pub(in crate::compiler::native_ir) fn has_direct_source_layout(
    declaration: &CoreConstructorDecl,
) -> bool {
    let Some(implementation) = &declaration.implementation else {
        return false;
    };
    if declaration.vararg.is_some()
        || declaration.min_arity != declaration.params.len()
        || !implementation.generic_params.is_empty()
        || implementation.defaults.iter().any(Option::is_some)
    {
        return false;
    }
    let Some(mut body) = declaration.physical_body.as_ref() else {
        return false;
    };
    if let CoreExpr::Cast { expr, .. } = body {
        body = expr;
    }
    fn is_checked_singleton_tag(expr: &CoreExpr) -> bool {
        matches!(expr, CoreExpr::Atom(_))
    }
    let forwards_variant = match body {
        body if is_checked_singleton_tag(body) => declaration.params.is_empty(),
        CoreExpr::Tuple(items) => items.len() == declaration.params.len() + 1
            && items.first().is_some_and(is_checked_singleton_tag)
            && items.iter().skip(1).zip(&declaration.params).all(
                |(item, parameter)| matches!(item, CoreExpr::Var(name) if name == &parameter.name),
            ),
        _ => false,
    };
    let Some(return_type) = declaration.core_return_type.as_ref() else {
        return false;
    };
    forwards_variant
        && super::native_type(Some(return_type), &declaration.return_type)
            .is_some_and(super::NativeType::is_managed_reference)
        && direct_source_variant_position(declaration, return_type).is_some()
}

pub(in crate::compiler::native_ir) fn direct_source_variant_position(
    declaration: &CoreConstructorDecl,
    return_type: &CoreType,
) -> Option<(u32, u32)> {
    let tag = direct_source_tag(declaration)?;
    let CoreType::Union(variants) = return_type else {
        return None;
    };
    let index = variants.iter().position(|variant| match variant {
        CoreType::AtomLiteral(candidate) => candidate == tag,
        CoreType::Tuple(items) => matches!(
            items.first(),
            Some(CoreTupleTypeElem::Type(CoreType::AtomLiteral(candidate)))
                | Some(CoreTupleTypeElem::Field {
                    ty: CoreType::AtomLiteral(candidate),
                    ..
                }) if candidate == tag
        ),
        _ => false,
    })?;
    Some((
        u32::try_from(index).ok()?,
        u32::try_from(variants.len()).ok()?,
    ))
}

pub(in crate::compiler::native_ir) fn direct_source_variant_name(
    declaration: &CoreConstructorDecl,
) -> Option<String> {
    let tag = direct_source_tag(declaration)?;
    Some(match tag {
        "none" => "None".to_string(),
        "error" => "Err".to_string(),
        value => {
            let mut chars = value.chars();
            chars.next()?.to_uppercase().chain(chars).collect()
        }
    })
}

fn direct_source_tag(declaration: &CoreConstructorDecl) -> Option<&str> {
    let mut body = declaration.physical_body.as_ref()?;
    if let CoreExpr::Cast { expr, .. } = body {
        body = expr;
    }
    match body {
        CoreExpr::Atom(tag) => Some(tag),
        CoreExpr::Tuple(items) => match items.first()? {
            CoreExpr::Atom(tag) => Some(tag),
            _ => None,
        },
        _ => None,
    }
}

pub(in crate::compiler::native_ir) fn direct_source_field_names(
    return_type: &CoreType,
    discriminant: u32,
) -> Option<Vec<Option<String>>> {
    let CoreType::Union(variants) = return_type else {
        return None;
    };
    let variant = variants.get(usize::try_from(discriminant).ok()?)?;
    match variant {
        CoreType::AtomLiteral(_) => Some(Vec::new()),
        CoreType::Tuple(items) => Some(
            items
                .iter()
                .skip(1)
                .map(|item| match item {
                    CoreTupleTypeElem::Type(_) => None,
                    CoreTupleTypeElem::Field { name, .. } => Some(name.clone()),
                })
                .collect(),
        ),
        _ => None,
    }
}
