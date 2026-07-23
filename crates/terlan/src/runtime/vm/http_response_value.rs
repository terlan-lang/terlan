//! Typed result envelope used only by direct fixed-owner HTTP calls.

use super::ReplValue;

/// Owned non-file response copied from an actor heap before that request heap
/// is released. Protocol validation remains the serve boundary's authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmAotHttpResponse {
    pub(crate) kind: i64,
    pub(crate) status: i64,
    pub(crate) payload: String,
    pub(crate) headers: Vec<(String, String)>,
}

/// Result of a direct HTTP call. Unsupported response kinds retain the exact
/// generic path rather than changing their semantics.
#[derive(Debug)]
pub(crate) enum VmHttpCallResult {
    Response(VmAotHttpResponse),
    Generic(ReplValue),
}
