//! Structured failures from C++ metadata validation and binding generation.

use terlan_runtime_abi::{BoundaryError, ErrorDomain};

/// Preserves the binding diagnostic's code and command ownership through helpers.
#[derive(Debug)]
pub(in crate::commands::bind) struct CppBindingError(BoundaryError);

impl std::fmt::Display for CppBindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for CppBindingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl std::ops::Deref for CppBindingError {
    type Target = str;

    fn deref(&self) -> &str {
        self.0.context()
    }
}

impl From<String> for CppBindingError {
    fn from(message: String) -> Self {
        Self(BoundaryError::message(
            ErrorDomain::CommandExecution,
            "generate C++ bindings",
            message,
        ))
    }
}

impl From<BoundaryError> for CppBindingError {
    fn from(error: BoundaryError) -> Self {
        Self(error)
    }
}

impl From<&str> for CppBindingError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

impl From<CppBindingError> for String {
    fn from(error: CppBindingError) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
#[path = "error_test.rs"]
mod tests;
