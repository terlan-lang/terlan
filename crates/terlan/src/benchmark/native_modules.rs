#![forbid(unsafe_code)]
#![allow(dead_code)]

//! Native adapter module root for benchmark binaries.
//!
//! The production native module root includes every adapter, including vector
//! adapters measured by benchmarks plus the value adapters required by the
//! shared VM and NativeBoundary runtime.

#[path = "../runtime/native/base64.rs"]
pub mod base64;

#[path = "../runtime/native/json.rs"]
pub mod json;

#[path = "../runtime/native/md5.rs"]
pub mod md5;

#[path = "../runtime/native/http.rs"]
pub mod http;

#[path = "../runtime/native/postgres.rs"]
pub mod postgres;

#[path = "../runtime/native/path.rs"]
pub mod path;

#[path = "../runtime/native/random.rs"]
pub mod random;

#[path = "../runtime/native/regex.rs"]
pub mod regex;

#[path = "../runtime/native/uri.rs"]
pub mod uri;

#[path = "../runtime/native/vector.rs"]
pub mod vector;
