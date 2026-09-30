//! Maintained encoding libraries behind package-owned value bindings.

#![forbid(unsafe_code)]

mod base64;
mod bindings;
mod md5;

pub use bindings::{
    DECODE_BYTES, DECODE_TEXT, DECODE_URL_BYTES, DECODE_URL_TEXT, ENCODE, ENCODE_BYTES, ENCODE_URL,
    ENCODE_URL_BYTES, MD5,
};
