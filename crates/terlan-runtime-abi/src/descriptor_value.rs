//! Borrowed inspection for package-owned source descriptors.

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

/// Lets packages inspect data without depending on VM value representation.
/// This is not serialization or permission to invoke a callback. Hosts retain
/// callback validation, code-generation ownership, and execution authority.
pub trait DescriptorValue: Sized {
    /// Borrows the structural view of this value.
    fn descriptor_view(&self) -> DescriptorView<'_, Self>;
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
}
