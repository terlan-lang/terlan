//! HTTP admission policy descriptors; scheduling is supplied by the host.

use crate::route_pattern::WebRouteError;
/// Policy applied when the VM HTTP worker queue reaches its bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverloadPolicy {
    Queue,
    Reject,
    Spill,
}

/// Validated source-level overload configuration owned by the HTTP package.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OverloadConfig {
    pub policy: OverloadPolicy,
    pub max_pending: usize,
}

impl OverloadConfig {
    /// Validates one bounded pending-work configuration.
    pub fn new(policy: OverloadPolicy, max_pending: usize) -> Result<Self, WebRouteError> {
        if max_pending == 0 {
            return Err("max_pending must be greater than 0".into());
        }
        Ok(Self {
            policy,
            max_pending,
        })
    }
}
