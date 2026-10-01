//! Typed failures from release discovery and installation.

/// A failed self-update operation with its original rendered diagnostic.
#[derive(Debug)]
pub(super) struct UpdateError {
    diagnostic: terlan_runtime_abi::BoundaryError,
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.diagnostic.fmt(formatter)
    }
}

impl std::error::Error for UpdateError {}

impl std::ops::Deref for UpdateError {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.diagnostic.context()
    }
}

impl From<String> for UpdateError {
    fn from(message: String) -> Self {
        Self {
            diagnostic: terlan_runtime_abi::BoundaryError::message(
                terlan_runtime_abi::ErrorDomain::CommandExecution,
                "update installed Terlan toolchain",
                message,
            ),
        }
    }
}

impl From<&str> for UpdateError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
