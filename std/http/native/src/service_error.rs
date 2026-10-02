//! Typed diagnostics at the package-owned HTTP service boundary.

use std::{error::Error, fmt};
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

/// A service policy, TLS, or WebSocket failure with its original diagnostic code.
#[derive(Debug)]
pub struct ServiceError(BoundaryError);

impl ServiceError {
    /// Returns the stable diagnostic code retained at the package boundary.
    pub fn code(&self) -> &str {
        self.0.code()
    }
}

impl From<String> for ServiceError {
    fn from(message: String) -> Self {
        Self(BoundaryError::message(
            ErrorDomain::NativeBoundary,
            "HTTP service",
            message,
        ))
    }
}

impl From<&str> for ServiceError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

impl From<ServiceError> for String {
    fn from(error: ServiceError) -> Self {
        error.to_string()
    }
}

impl fmt::Display for ServiceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for ServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}
