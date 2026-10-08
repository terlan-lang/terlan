//! Compatibility import for the HTTP package's route declaration contract.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use terlan_http_native::route_pattern::{route_param_types, validate_route_pattern};
