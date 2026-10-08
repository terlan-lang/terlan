//! Structural package inspection keeps callable execution in the host.
use super::ReplValue;
use terlan_runtime_abi::{DescriptorValue, DescriptorView, OwnedDescriptor};

impl DescriptorValue for ReplValue {
    fn descriptor_view(&self) -> DescriptorView<'_, Self> {
        match self {
            Self::Int(value) => DescriptorView::Int(*value),
            Self::String(value) => DescriptorView::String(value),
            Self::StringBytes(value) => std::str::from_utf8(value)
                .map(DescriptorView::String)
                .unwrap_or(DescriptorView::Opaque),
            Self::Atom(value) => DescriptorView::Atom(value),
            Self::Record { name, fields } => DescriptorView::Record(name, fields),
            Self::Tuple(values) => DescriptorView::Tuple(values),
            Self::List(values) => DescriptorView::List(values),
            _ => DescriptorView::Opaque,
        }
    }

    fn into_descriptor(self) -> OwnedDescriptor<Self> {
        match self {
            Self::Unit => OwnedDescriptor::Unit,
            Self::Int(value) => OwnedDescriptor::Int(value),
            Self::String(value) => OwnedDescriptor::String(value),
            Self::StringBytes(value) => std::str::from_utf8(&value)
                .map(|text| OwnedDescriptor::String(text.to_owned()))
                .unwrap_or(OwnedDescriptor::Opaque),
            Self::Atom(value) => OwnedDescriptor::Atom(value),
            Self::Record { name, fields } => OwnedDescriptor::Record(name, fields),
            Self::Tuple(values) => OwnedDescriptor::Tuple(values),
            Self::List(values) => OwnedDescriptor::List(values),
            _ => OwnedDescriptor::Opaque,
        }
    }
}

#[cfg(test)]
#[path = "value_descriptor_test.rs"]
mod tests;
