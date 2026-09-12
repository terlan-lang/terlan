//! Closed tagged-union constructor selection and payload pattern planning.

use super::{
    encode_managed_variant_is_operation, managed_semantic, merge, native_core_type, pattern_plan,
    project, tuple_element_type, Arc, CorePattern, CoreTupleTypeElem, CoreType,
    NativeConstructorLayouts, NativeExpr, NativeType, PatternPlan,
};

/// Closed-union constructor identity and checked payload fields.
pub(super) struct TaggedUnionPattern<'a> {
    /// Source constructor name used in arity diagnostics.
    pub(super) name: &'a str,
    /// Payload patterns in declaration order.
    pub(super) fields: &'a [CorePattern],
    /// Runtime variant tag in the closed union.
    pub(super) discriminant: u32,
    /// Checked types corresponding to each payload pattern.
    pub(super) field_types: &'a [CoreType],
}

/// Lowers a checked tagged-union pattern into a discriminant and field plan.
pub(super) fn tagged_union_constructor_plan(
    pattern: TaggedUnionPattern<'_>,
    value: NativeExpr,
    value_type: NativeType,
    constructors: &NativeConstructorLayouts,
    depth: usize,
) -> Result<PatternPlan, String> {
    let TaggedUnionPattern {
        name,
        fields: patterns,
        discriminant,
        field_types: fields,
    } = pattern;
    if patterns.len() != fields.len() {
        return Err(format!(
            "error[native_ir.union_pattern_arity]: `{name}` expects {} fields",
            fields.len()
        ));
    }
    let semantic = managed_semantic(value_type)?;
    let mut plans = vec![PatternPlan {
        predicate: NativeExpr::ManagedOperation {
            encoded: Arc::from(encode_managed_variant_is_operation(semantic, discriminant)),
            args: vec![value.clone()],
        },
        bindings: Vec::new(),
    }];
    for (index, (pattern, field)) in patterns.iter().zip(fields).enumerate() {
        let field_type = native_core_type(field)?;
        plans.push(pattern_plan(
            pattern,
            project(value.clone(), semantic, index, field_type)?,
            field_type,
            Some(field),
            constructors,
            depth + 1,
        )?);
    }
    merge(plans)
}

/// Resolves a constructor's discriminant and payload in a closed union.
pub(super) fn tagged_union_constructor(
    name: &str,
    core_type: Option<&CoreType>,
) -> Option<(u32, u32, Vec<CoreType>)> {
    let CoreType::Union(variants) = core_type? else {
        return None;
    };
    let expected = match name.rsplit('.').next()? {
        "Err" => "error",
        other => return tagged_union_by_constructor_name(other, variants),
    };
    tagged_union_by_atom(expected, variants)
}

fn tagged_union_by_constructor_name(
    name: &str,
    variants: &[CoreType],
) -> Option<(u32, u32, Vec<CoreType>)> {
    let mut chars = name.chars();
    let expected = chars
        .next()?
        .to_lowercase()
        .chain(chars)
        .collect::<String>();
    tagged_union_by_atom(&expected, variants)
}

/// Finds a tuple-headed union variant by its structural atom identity.
pub(super) fn tagged_union_by_atom(
    expected: &str,
    variants: &[CoreType],
) -> Option<(u32, u32, Vec<CoreType>)> {
    variants.iter().enumerate().find_map(|(index, variant)| {
        let CoreType::Tuple(elements) = variant else {
            return None;
        };
        let (first, fields) = elements.split_first()?;
        let atom = match first {
            CoreTupleTypeElem::Type(CoreType::AtomLiteral(atom))
            | CoreTupleTypeElem::Field {
                ty: CoreType::AtomLiteral(atom),
                ..
            } => atom,
            _ => return None,
        };
        if atom != expected {
            return None;
        }
        Some((
            u32::try_from(index).ok()?,
            u32::try_from(variants.len()).ok()?,
            fields.iter().map(tuple_element_type).cloned().collect(),
        ))
    })
}
