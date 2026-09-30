//! Exact copied-value contracts for maintained codecs.

use terlan_runtime_abi::{BoundaryError, ErrorDomain, NativeBinding, NativeValue};

use crate::base64::{self, DecodeFailure};

macro_rules! codec_binding {
    ($name:ident, $operation:literal, $input:ident, $call:expr) => {
        pub const $name: NativeBinding = NativeBinding {
            operation: $operation,
            arity: 1,
            invoke: |args| {
                let [NativeValue::$input(value)] = args else {
                    return Err(BoundaryError::message(
                        ErrorDomain::NativeBoundary,
                        "encoding arguments",
                        concat!(
                            "error[dispatch.type]: ",
                            $operation,
                            " expects one ",
                            stringify!($input)
                        ),
                    ));
                };
                Ok(($call)(value))
            },
        };
    };
}

codec_binding!(
    ENCODE,
    "std.encoding.base64.encode",
    String,
    |v: &String| base64::encode(v).into()
);
codec_binding!(
    ENCODE_URL,
    "std.encoding.base64.encode_url",
    String,
    |v: &String| base64::encode_url(v).into()
);
codec_binding!(
    ENCODE_BYTES,
    "std.encoding.base64.encode_bytes",
    Bytes,
    |v: &Vec<u8>| base64::encode_bytes(v).into()
);
codec_binding!(
    ENCODE_URL_BYTES,
    "std.encoding.base64.encode_url_bytes",
    Bytes,
    |v: &Vec<u8>| base64::encode_url_bytes(v).into()
);
codec_binding!(
    DECODE_TEXT,
    "std.encoding.base64.decode_text",
    String,
    |v: &String| decoded(base64::decode(v).map(NativeValue::String))
);
codec_binding!(
    DECODE_URL_TEXT,
    "std.encoding.base64.decode_url_text",
    String,
    |v: &String| decoded(base64::decode_url(v).map(NativeValue::String))
);
codec_binding!(
    DECODE_BYTES,
    "std.encoding.base64.decode_octets",
    String,
    |v: &String| decoded(base64::decode_bytes(v).map(NativeValue::Bytes))
);
codec_binding!(
    DECODE_URL_BYTES,
    "std.encoding.base64.decode_url_octets",
    String,
    |v: &String| decoded(base64::decode_url_bytes(v).map(NativeValue::Bytes))
);
codec_binding!(MD5, "std.encoding.md5.digest", String, |v: &String| {
    crate::md5::digest(v).into()
});

// The source module chooses public error codes and offset policy. The binding
// only distinguishes the failed maintained decoder and preserves its message.
fn decoded(value: Result<NativeValue, DecodeFailure>) -> NativeValue {
    value
        .map_err(|error| {
            let (utf8, message) = match error {
                DecodeFailure::Encoding(error) => (false, error.to_string()),
                DecodeFailure::Utf8(error) => (true, error.to_string()),
            };
            (NativeValue::Bool(utf8), message)
        })
        .into()
}

#[cfg(test)]
#[path = "bindings_test.rs"]
mod tests;
