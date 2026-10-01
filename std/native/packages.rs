//! Built-in package composition, separate from compiler and VM semantics.

pub static VALUE_BINDINGS: &[terlan_runtime_abi::NativeBinding] = &[
    terlan_net_native::PARSE,
    terlan_net_native::QUERY_PAIRS,
    terlan_http_native::SET_HEADER_WITH_OPTIONS,
    terlan_http_native::ENCODE_EVENT,
    terlan_encoding_native::ENCODE,
    terlan_encoding_native::ENCODE_URL,
    terlan_encoding_native::ENCODE_BYTES,
    terlan_encoding_native::ENCODE_URL_BYTES,
    terlan_encoding_native::DECODE_TEXT,
    terlan_encoding_native::DECODE_URL_TEXT,
    terlan_encoding_native::DECODE_BYTES,
    terlan_encoding_native::DECODE_URL_BYTES,
    terlan_encoding_native::MD5,
    terlan_crypto_native::VERIFY_ED25519,
    terlan_crypto_native::SHA256,
    terlan_crypto_native::SHA256_BYTES,
    terlan_crypto_native::SHA256_FRAMED,
    terlan_crypto_native::SHA256_DOMAIN_FRAMED,
    terlan_crypto_native::SHA256_NUL_SEPARATED,
    terlan_time_native::UNIX_TIME_NS,
    terlan_time_native::MONOTONIC_TIME_NS,
];

/// Looks up an exact registered operation, without interpreting its namespace.
pub fn value_binding(operation: &str) -> Option<&'static terlan_runtime_abi::NativeBinding> {
    VALUE_BINDINGS
        .iter()
        .find(|binding| binding.operation == operation)
}

/// Package contracts requiring resource-aware execution, not value transport.
pub static RESOURCE_OPERATIONS: &[&[terlan_runtime_abi::NativeResourceOperation]] =
    &[terlan_data_native::RESOURCE_OPERATIONS];

/// Looks up a resource contract without granting any execution capability.
pub fn resource_operation(
    operation: &str,
) -> Option<&'static terlan_runtime_abi::NativeResourceOperation> {
    RESOURCE_OPERATIONS
        .iter()
        .flat_map(|operations| operations.iter())
        .find(|contract| contract.operation == operation)
}

/// Declared context-call arity, not an execution grant. The package's callable
/// catalog is the authority; no storage is created or consulted by this lookup.
pub fn context_operation_arity(operation: &str) -> Option<usize> {
    terlan_http_native::session_bindings::bindings::<
        dyn terlan_http_native::session_bindings::SessionStorage,
    >()
    .iter()
    .find(|binding| binding.operation == operation)
    .map(|binding| binding.arity())
}

#[cfg(test)]
#[path = "packages_test.rs"]
mod tests;
