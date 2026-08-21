//! Typed failures shared by Registry command, transport, trust, and resolution code.

use std::fmt;
use std::ops::Deref;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct RegistryError(String);

pub(super) type RegistryResult<T> = Result<T, RegistryError>;

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for RegistryError {}

impl Deref for RegistryError {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<String> for RegistryError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

impl From<&str> for RegistryError {
    fn from(message: &str) -> Self {
        Self(message.to_owned())
    }
}

impl From<RegistryError> for String {
    fn from(error: RegistryError) -> Self {
        error.0
    }
}
