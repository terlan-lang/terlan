//! Descriptor-authorized public aggregate shapes at the native boundary.

use super::managed_allocation_error;
use crate::runtime::native_image::managed::{
    managed_binary_semantic_id, managed_bytes_semantic_id, managed_string_semantic_id,
    ManagedAggregateDescriptor, ManagedAggregateKind, ManagedCollectionKind, ManagedFieldType,
    ManagedLayoutRegistry, SemanticTypeId,
};
use crate::runtime::vm::ReplValue;

/// Selects exactly one admitted active layout and borrows its public fields.
pub(super) fn select_layout<'layout, 'value>(
    layouts: &'layout ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    value: &'value ReplValue,
) -> Result<(&'layout ManagedAggregateDescriptor, PublicFields<'value>), String> {
    if let ReplValue::Atom(identity) = value {
        let index = layouts
            .atom_index(identity)
            .map_err(managed_allocation_error)?;
        if let Ok(descriptor) = crate::runtime::native_image::managed::immediate_variant(
            layouts,
            semantic,
            i64::from(index.get()),
        ) {
            return Ok((descriptor, PublicFields::Positional(&[])));
        }
    }
    let mut matched = None;
    let mut match_count = 0;
    for layout in layouts.layouts(semantic) {
        if let Some(fields) =
            public_fields(layout, value).or_else(|| union_wrapper_fields(layouts, layout, value, 0))
        {
            match_count += 1;
            if matched.is_none() {
                matched = Some((layout.as_ref(), fields));
            }
        }
    }
    match match_count {
        1 => Ok(matched.expect("one checked layout")),
        0 => {
            let candidates = layouts
                .layouts(semantic)
                .iter()
                .map(|layout| layout.canonical_type())
                .collect::<Vec<_>>();
            Err(format!(
                "error[execution_shard.managed_layout]: no admitted fixed layout matches `{value:?}` for semantic {:?}; candidates {candidates:?}",
                semantic.bytes()
            ))
        }
        count => Err(format!(
            "error[execution_shard.managed_layout]: public value ambiguously matches {count} admitted layouts"
        )),
    }
}

/// Borrowed aggregate fields without allocating a temporary reference vector.
#[derive(Clone, Copy)]
pub(super) enum PublicFields<'a> {
    Positional(&'a [ReplValue]),
    Named(&'a [(String, ReplValue)]),
    Wrapped(&'a ReplValue),
}

impl<'a> PublicFields<'a> {
    /// Returns the exact public field count.
    pub(super) fn len(self) -> usize {
        match self {
            Self::Positional(values) => values.len(),
            Self::Named(fields) => fields.len(),
            Self::Wrapped(_) => 1,
        }
    }

    /// Borrows one descriptor-ordered field.
    pub(super) fn value(self, index: usize) -> &'a ReplValue {
        match self {
            Self::Positional(values) => &values[index],
            Self::Named(fields) => &fields[index].1,
            Self::Wrapped(value) if index == 0 => value,
            Self::Wrapped(_) => unreachable!("checked wrapped field index"),
        }
    }
}

/// Treats a boxed union's single payload slot as a transparent public boundary.
fn union_wrapper_fields<'a>(
    layouts: &ManagedLayoutRegistry,
    descriptor: &ManagedAggregateDescriptor,
    value: &'a ReplValue,
    depth: usize,
) -> Option<PublicFields<'a>> {
    if depth >= 32
        || !descriptor
            .variant_name()
            .is_some_and(|name| name.starts_with("$native.union."))
        || descriptor.fields().len() != 1
        || descriptor.fields()[0].name() != Some("value")
        || !public_value_matches(
            layouts,
            descriptor.fields()[0].field_type(),
            value,
            depth + 1,
        )
    {
        return None;
    }
    Some(PublicFields::Wrapped(value))
}

/// Selects the boxed union member from its admitted physical field type.
fn public_value_matches(
    layouts: &ManagedLayoutRegistry,
    field_type: ManagedFieldType,
    value: &ReplValue,
    depth: usize,
) -> bool {
    match (field_type, value) {
        (ManagedFieldType::Unit, ReplValue::Unit)
        | (ManagedFieldType::Bool, ReplValue::Bool(_))
        | (ManagedFieldType::Int, ReplValue::Int(_))
        | (ManagedFieldType::Float, ReplValue::Float(_))
        | (ManagedFieldType::Atom, ReplValue::Atom(_)) => true,
        (ManagedFieldType::Reference(semantic), value) => {
            if semantic == managed_string_semantic_id() {
                return matches!(value, ReplValue::String(_));
            }
            if semantic == managed_bytes_semantic_id() {
                return matches!(value, ReplValue::Bytes(_));
            }
            if semantic == managed_binary_semantic_id() {
                return matches!(value, ReplValue::BitString(_));
            }
            if let Some(collection) = layouts.collection(semantic) {
                return matches!(
                    (collection.kind(), value),
                    (ManagedCollectionKind::List, ReplValue::List(_))
                        | (ManagedCollectionKind::Map, ReplValue::Map(_))
                        | (ManagedCollectionKind::Set, ReplValue::Set(_))
                );
            }
            if let ReplValue::Atom(identity) = value {
                let Ok(index) = layouts.atom_index(identity) else {
                    return false;
                };
                return crate::runtime::native_image::managed::immediate_variant(
                    layouts,
                    semantic,
                    i64::from(index.get()),
                )
                .is_ok();
            }
            layouts.layouts(semantic).iter().any(|candidate| {
                public_fields(candidate, value).is_some()
                    || union_wrapper_fields(layouts, candidate, value, depth).is_some()
            })
        }
        _ => false,
    }
}

/// Matches one public aggregate shape against an admitted active descriptor.
fn public_fields<'a>(
    descriptor: &ManagedAggregateDescriptor,
    value: &'a ReplValue,
) -> Option<PublicFields<'a>> {
    match (descriptor.kind(), value) {
        (ManagedAggregateKind::Tuple, ReplValue::Tuple(values))
        | (ManagedAggregateKind::Array, ReplValue::List(values))
            if values.len() == descriptor.fields().len() =>
        {
            Some(PublicFields::Positional(values))
        }
        (ManagedAggregateKind::Record, ReplValue::Record { name, fields })
            if type_name_matches(descriptor.canonical_type(), name)
                && named_fields_match(descriptor, fields) =>
        {
            Some(PublicFields::Named(fields))
        }
        (ManagedAggregateKind::Constructor, ReplValue::Record { name, fields })
            if descriptor.variant_name() == Some(name.as_str())
                && named_fields_match(descriptor, fields) =>
        {
            Some(PublicFields::Named(fields))
        }
        _ => None,
    }
}

/// Reports whether named public fields exactly preserve descriptor order and identity.
fn named_fields_match(
    descriptor: &ManagedAggregateDescriptor,
    fields: &[(String, ReplValue)],
) -> bool {
    fields.len() == descriptor.fields().len()
        && descriptor
            .fields()
            .iter()
            .zip(fields)
            .all(|(expected, (actual, _))| expected.name() == Some(actual.as_str()))
}

/// Accepts a canonical record identity or its unqualified final segment.
pub(crate) fn type_name_matches(canonical: &str, public: &str) -> bool {
    let nominal = canonical
        .strip_prefix("Named(")
        .and_then(|name| name.strip_suffix(')'))
        .unwrap_or(canonical);
    nominal == public || nominal.rsplit('.').next() == Some(public)
}

/// Rebuilds one public aggregate while retaining source field identities.
pub(super) fn public_aggregate(
    descriptor: &ManagedAggregateDescriptor,
    mut values: Vec<ReplValue>,
) -> ReplValue {
    if descriptor
        .variant_name()
        .is_some_and(|name| name.starts_with("$native.union."))
        && values.len() == 1
        && descriptor.fields().len() == 1
        && descriptor.fields()[0].name() == Some("value")
    {
        return values.pop().expect("checked one closed union payload");
    }
    match descriptor.kind() {
        ManagedAggregateKind::Tuple => ReplValue::Tuple(values),
        ManagedAggregateKind::Array => ReplValue::List(values),
        ManagedAggregateKind::Record | ManagedAggregateKind::Constructor => {
            let name = descriptor
                .variant_name()
                .unwrap_or_else(|| {
                    descriptor
                        .canonical_type()
                        .strip_prefix("Named(")
                        .and_then(|name| name.strip_suffix(')'))
                        .unwrap_or(descriptor.canonical_type())
                        .rsplit('.')
                        .next()
                        .expect("canonical aggregate identity is nonempty")
                })
                .to_string();
            let fields = descriptor
                .fields()
                .iter()
                .zip(values)
                .map(|(field, value)| (field.name().unwrap_or("_").to_string(), value))
                .collect();
            ReplValue::Record { name, fields }
        }
    }
}
