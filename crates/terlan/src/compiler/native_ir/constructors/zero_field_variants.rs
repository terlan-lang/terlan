//! Managed representation recovery for nullary constructor variants.

use crate::terlan_typeck::CoreExpr;

use super::{NativeConstructorLayout, NativeConstructorLayouts, NativeExpr, NativeType};

/// Reifies an atom-form zero-field variant when a containing aggregate field
/// requires the managed union representation.
pub(in crate::compiler::native_ir) fn lower_zero_field_managed_variant(
    argument: &CoreExpr,
    expected: NativeType,
    layouts: &NativeConstructorLayouts,
) -> Result<Option<NativeExpr>, String> {
    Ok(
        zero_field_managed_variant_layout(argument, expected, layouts)?.map(|layout| {
            NativeExpr::Construct {
                descriptor: layout.descriptor.clone(),
                encoded_layout: layout.encoded_layout.clone(),
                fields: Vec::new(),
            }
        }),
    )
}

/// Shares checked nullary variant selection between type inference and lowering.
pub(in crate::compiler::native_ir) fn zero_field_managed_variant_layout<'a>(
    argument: &CoreExpr,
    expected: NativeType,
    layouts: &'a NativeConstructorLayouts,
) -> super::super::NativeIrResult<Option<&'a NativeConstructorLayout>> {
    let NativeType::ManagedRef(_) = expected else {
        return Ok(None);
    };
    let (identity, qualified) = match argument {
        CoreExpr::Atom(identity) => (identity, None),
        CoreExpr::Var(identity)
            if identity
                .rsplit('.')
                .next()
                .and_then(|name| name.chars().next())
                .is_some_and(|first| first.is_ascii_uppercase()) =>
        {
            (identity, None)
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
            ..
        } if args.is_empty() => (constructor, constructor_identity.as_deref()),
        _ => return Ok(None),
    };
    if let Some(layout) = qualified
        .and_then(|qualified| layouts.get(&(qualified.to_string(), 0)))
        .or_else(|| layouts.get(&(identity.to_string(), 0)))
        .filter(|layout| layout.result == expected && layout.parameters.is_empty())
    {
        return Ok(Some(layout));
    }
    let identity = identity.rsplit('.').next().unwrap_or(identity);
    let mut candidates = layouts.values().filter(|layout| {
        layout.result == expected
            && layout.parameters.is_empty()
            && layout
                .descriptor
                .variant_name()
                .is_some_and(|variant| variant.eq_ignore_ascii_case(identity))
    });
    let Some(candidate) = candidates.next() else {
        return Ok(None);
    };
    if candidates.any(|other| other.encoded_layout != candidate.encoded_layout) {
        return Err(format!(
            "error[native_ir.constructor_variant]: atom `{identity}` has ambiguous managed layouts"
        )
        .into());
    }
    Ok(Some(candidate))
}
