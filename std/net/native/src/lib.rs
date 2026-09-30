#![forbid(unsafe_code)]

//! Package-owned maintained URL and TLS codecs, independent of compiler/VM state.

pub mod tls;
pub mod tls_stream;

/// Decodes form-url-encoded query pairs in wire order using the maintained URL codec.
pub fn query_pairs(query: &str) -> Vec<(String, String)> {
    url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect()
}

use terlan_runtime_abi::{native_record, BoundaryError, ErrorDomain, NativeBinding, NativeValue};
use url::Url;

/// The sole native operation needed by the Terlan URI implementation.
pub const PARSE: NativeBinding = NativeBinding {
    operation: "std.net.uri.parse_parts",
    arity: 1,
    invoke: parse_parts,
};

/// Parses one string into copied components or a parser error message.
pub fn parse_parts(arguments: &[NativeValue]) -> Result<NativeValue, BoundaryError> {
    let [NativeValue::String(text)] = arguments else {
        return Err(BoundaryError::message(
            ErrorDomain::NativeBoundary,
            "URI parser arguments",
            "error[native_package.arguments]: URI parsing requires one String",
        ));
    };
    Ok(Url::parse(text)
        .map(
            |value| native_record!(Uri, value, { as_str, scheme, host_str, path, query, fragment }),
        )
        .map_err(|error| error.to_string())
        .into())
}

#[cfg(test)]
#[path = "lib_test.rs"]
mod tests;
