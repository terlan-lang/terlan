//! Concrete, descriptor-owned storage for untagged closed unions.
use super::{NativeConstructorLayouts, NativeExpr, NativeType};
use crate::runtime::native_image::managed::{encode_aggregate_layout, ManagedAggregateDescriptor};
use crate::terlan_typeck::{CoreExpr, CoreTupleTypeElem, CoreType};
use std::collections::HashMap;
use std::sync::Arc;

pub(super) const VARIANT_PREFIX: &str = "$native.union.";

/// Tagged sums retain their established constructor ABI; enum-only unions stay atoms.
pub(super) fn is_boxed(ty: &CoreType) -> bool {
    let CoreType::Union(variants) = ty else {
        return false;
    };
    if variants
        .iter()
        .all(|v| matches!(v, CoreType::Atom | CoreType::AtomLiteral(_)))
    {
        return false;
    }
    !variants.iter().all(|v| match v {
        CoreType::AtomLiteral(_) => true,
        CoreType::Tuple(items) => matches!(
            items.first(),
            Some(CoreTupleTypeElem::Type(CoreType::AtomLiteral(_)))
                | Some(CoreTupleTypeElem::Field {
                    ty: CoreType::AtomLiteral(_),
                    ..
                })
        ),
        _ => false,
    })
}

/// One payload layout per physical member kind, preserving the union's identity.
pub(super) fn layouts(
    ty: &CoreType,
) -> Result<Vec<(NativeType, Arc<ManagedAggregateDescriptor>)>, String> {
    if !is_boxed(ty) {
        return Ok(Vec::new());
    }
    let CoreType::Union(members) = ty else {
        unreachable!()
    };
    let mut kinds = Vec::new();
    for member in members {
        let kind = super::native_type(Some(member), &member.contract_text()).ok_or_else(|| {
            format!(
                "error[native_ir.union_member]: unsupported {}",
                member.contract_text()
            )
        })?;
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }
    let count = u32::try_from(kinds.len()).map_err(|_| "union has too many native member kinds")?;
    let canonical = super::expression::managed_semantic_contract(ty);
    kinds
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            let descriptor = ManagedAggregateDescriptor::constructor(
                &canonical,
                &format!("{VARIANT_PREFIX}{index}"),
                index as u32,
                count,
                vec![(
                    Some("value".to_string()),
                    super::constructors::managed_field_type(kind)?,
                )],
            )
            .map_err(|error| error.to_string())?;
            Ok((kind, Arc::new(descriptor)))
        })
        .collect()
}

pub(super) fn lower(
    value: &CoreExpr,
    target: &CoreType,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<Option<NativeExpr>, String> {
    if !is_boxed(target) {
        return Ok(None);
    }
    // Retain the checked member's tuple fields; scalar ABI inference alone loses
    // singleton enums and can no longer recover a heterogeneous tuple's type.
    if let (CoreExpr::Tuple(items), CoreType::Union(members)) = (value, target) {
        let candidates = members
            .iter()
            .filter(|member| match member {
                CoreType::Tuple(fields) if fields.len() == items.len() => {
                    match (items.first(), fields.first()) {
                        (
                            Some(CoreExpr::Atom(actual)),
                            Some(CoreTupleTypeElem::Type(CoreType::AtomLiteral(expected))),
                        ) => actual == expected,
                        _ => false,
                    }
                }
                _ => false,
            })
            .collect::<Vec<_>>();
        if let [member] = candidates.as_slice() {
            let kind = super::native_type(Some(member), &member.contract_text())
                .ok_or("union tuple has no native representation")?;
            let (_, descriptor) = layouts(target)?
                .into_iter()
                .find(|(candidate, _)| *candidate == kind)
                .ok_or("union tuple layout is missing")?;
            let field = super::collection_values::lower_typed_value(
                value,
                member,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?;
            let encoded_layout = encode_aggregate_layout(&descriptor)
                .map_err(|error| error.to_string())?
                .into();
            return Ok(Some(NativeExpr::Construct {
                descriptor,
                encoded_layout,
                fields: vec![field],
            }));
        }
    }
    let source = super::infer_native_type_with_constructors(
        value,
        param_types,
        function_types,
        constructors,
    )
    .ok_or_else(|| format!("error[native_ir.union_source]: cannot infer {value:?}"))?;
    let expected = super::native_type(Some(target), &target.contract_text())
        .ok_or("union has no native identity")?;
    if source == expected {
        return super::lower_expr_with_constructors(
            value,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )
        .map(Some);
    }
    let (_, descriptor) = layouts(target)?
        .into_iter()
        .find(|(kind, _)| *kind == source)
        .ok_or_else(|| {
            format!(
                "error[native_ir.union_member]: {source:?} is not a native member of {}",
                target.contract_text()
            )
        })?;
    let field = super::lower_expr_with_constructors(
        value,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )?;
    let encoded_layout = encode_aggregate_layout(&descriptor)
        .map_err(|error| error.to_string())?
        .into();
    Ok(Some(NativeExpr::Construct {
        descriptor,
        encoded_layout,
        fields: vec![field],
    }))
}
