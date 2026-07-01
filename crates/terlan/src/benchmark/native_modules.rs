#![forbid(unsafe_code)]
#![allow(dead_code)]
#![allow(unused_imports)]

//! Minimal native adapter module root for benchmark binaries.
//!
//! The production native module root includes every adapter, including vector
//! adapters that depend on the SafeNative dispatch tree. The Postgres baseline
//! benchmark intentionally imports only the concrete adapters it measures.

#[path = "../runtime/native/json.rs"]
pub mod json;

#[path = "../runtime/native/postgres.rs"]
pub mod postgres;
