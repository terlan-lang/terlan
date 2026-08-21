//! Public Terlan Registry protocol contracts.

pub mod admission;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod fixtures;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod output;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod schema;
mod version;

pub mod model;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use version::latest_stable;
pub(crate) use version::{canonical_version, parse_requirement, requirement_matches};

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
struct ProtocolDocument {
    file_name: &'static str,
    value: serde_json::Value,
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use output::run_protocol_command;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub use output::ProtocolBundleSummary;

#[cfg(test)]
#[path = "protocol_test.rs"]
mod tests;
