//! Structural package inspection keeps callable execution in the host.
use super::ReplValue;
use terlan_runtime_abi::{DescriptorValue, DescriptorView};

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
}
