pub(super) fn native_symbol(module: &str, function: &str, arity: usize) -> String {
    let mut symbol = String::from("terlan_native_");
    for byte in format!("{module}.{function}/{arity}").bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'_' {
            symbol.push(char::from(byte));
        } else {
            symbol.push('_');
            symbol.push_str(&format!("{byte:02x}"));
        }
    }
    symbol
}

/// Returns the stable linker symbol for one compiler-generated continuation.
pub(super) fn native_continuation_symbol(identity: u64) -> String {
    format!("terlan_native_continuation_{identity:016x}")
}
