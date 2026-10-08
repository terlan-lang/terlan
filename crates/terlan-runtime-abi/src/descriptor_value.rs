//! Structural inspection for package-owned source descriptors.

/// A data view that deliberately leaves callbacks and resources opaque.
pub enum DescriptorView<'a, V> {
    /// A source integer.
    Int(i64),
    /// UTF-8 text.
    String(&'a str),
    /// A source singleton tag.
    Atom(&'a str),
    /// An ordinary source record.
    Record(&'a str, &'a [(String, V)]),
    /// A source tuple, including tagged unions.
    Tuple(&'a [V]),
    /// A source list.
    List(&'a [V]),
    /// A value which must be handled explicitly by its host.
    Opaque,
}

/// Consuming counterpart to `DescriptorView`, preserving owned allocations.
/// Opaque callbacks and resources are dropped, never exposed or invoked.
pub enum OwnedDescriptor<V> {
    Unit,
    Int(i64),
    String(String),
    Atom(String),
    Record(String, Vec<(String, V)>),
    Tuple(Vec<V>),
    List(Vec<V>),
    Opaque,
}

/// Lets packages inspect data without depending on VM value representation.
/// This is not serialization or permission to invoke a callback. Hosts retain
/// callback validation, code-generation ownership, and execution authority.
pub trait DescriptorValue: Sized {
    /// Borrows the structural view of this value.
    fn descriptor_view(&self) -> DescriptorView<'_, Self>;

    /// Transfers structural data without serializing it or cloning owned text.
    fn into_descriptor(self) -> OwnedDescriptor<Self>;
}

impl DescriptorValue for crate::NativeValue {
    fn descriptor_view(&self) -> DescriptorView<'_, Self> {
        match self {
            Self::Int(value) => DescriptorView::Int(*value),
            Self::String(value) => DescriptorView::String(value),
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
            Self::Atom(value) => OwnedDescriptor::Atom(value),
            Self::Record { name, fields } => OwnedDescriptor::Record(name, fields),
            Self::Tuple(values) => OwnedDescriptor::Tuple(values),
            Self::List(values) => OwnedDescriptor::List(values),
            _ => OwnedDescriptor::Opaque,
        }
    }
}

#[cfg(test)]
#[path = "descriptor_value_test.rs"]
mod tests;
