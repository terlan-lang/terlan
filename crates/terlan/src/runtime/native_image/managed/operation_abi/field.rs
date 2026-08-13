//! Managed-field conversion into the frozen native word ABI.

use super::super::ManagedFieldValue;

/// Converts one checked physical field into its native word representation.
pub(crate) fn field_word(value: ManagedFieldValue) -> u64 {
    match value {
        ManagedFieldValue::Unit => 0,
        ManagedFieldValue::Bool(value) => u64::from(value),
        ManagedFieldValue::Int(value) => u64::from_ne_bytes(value.to_ne_bytes()),
        ManagedFieldValue::Float(value) => value.to_bits(),
        ManagedFieldValue::Atom(value) => u64::from(value.get()),
        ManagedFieldValue::Reference(value) => value.encoded_abi_word(),
    }
}
