//! Descriptor-authorized public aggregate shapes at the native boundary.

use super::managed_allocation_error;
use crate::runtime::native_image::managed::{
    ManagedAggregateDescriptor, ManagedAggregateKind, ManagedLayoutRegistry, SemanticTypeId,
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
        let descriptor = crate::runtime::native_image::managed::immediate_variant(
            layouts,
            semantic,
            i64::from(index.get()),
        )
        .map_err(managed_allocation_error)?;
        return Ok((descriptor, PublicFields::Positional(&[])));
    }
    let mut matched = None;
    let mut match_count = 0;
    for layout in layouts.layouts(semantic) {
        if let Some(fields) = public_fields(layout, value) {
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
}

impl<'a> PublicFields<'a> {
    /// Returns the exact public field count.
    pub(super) fn len(self) -> usize {
        match self {
            Self::Positional(values) => values.len(),
            Self::Named(fields) => fields.len(),
        }
    }

    /// Borrows one descriptor-ordered field.
    pub(super) fn value(self, index: usize) -> &'a ReplValue {
        match self {
            Self::Positional(values) => &values[index],
            Self::Named(fields) => &fields[index].1,
        }
    }
}

/// Matches one public aggregate shape against an admitted active descriptor.
fn public_fields<'a>(
    descriptor: &ManagedAggregateDescriptor,
    value: &'a ReplValue,
) -> Option<PublicFields<'a>> {
    match (descriptor.kind(), value) {
        (ManagedAggregateKind::Tuple, ReplValue::Tuple(values))
        | (ManagedAggregateKind::FixedArray, ReplValue::List(values))
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
    values: Vec<ReplValue>,
) -> ReplValue {
    match descriptor.kind() {
        ManagedAggregateKind::Tuple => ReplValue::Tuple(values),
        ManagedAggregateKind::FixedArray => ReplValue::List(values),
        ManagedAggregateKind::Record | ManagedAggregateKind::Constructor => {
            let name = descriptor
                .variant_name()
                .unwrap_or_else(|| {
                    descriptor
                        .canonical_type()
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
