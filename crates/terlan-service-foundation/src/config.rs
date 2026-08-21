use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

/// Typed reference to a value declared by Terlan configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigRef<T> {
    name: String,
    #[serde(skip)]
    value_type: PhantomData<fn() -> T>,
}

impl<T> ConfigRef<T> {
    /// Declares a typed configuration reference with the given stable name.
    pub fn declared(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value_type: PhantomData,
        }
    }

    /// Returns the declared configuration identity without resolving its value.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Opaque typed reference. It exposes identity, never secret material or a
/// conversion into a log field.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecretRef {
    name: String,
}

impl SecretRef {
    /// Declares an opaque secret reference with the given stable name.
    pub fn declared(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
    /// Returns the secret identity without exposing secret material.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl std::fmt::Debug for SecretRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecretRef")
            .field("name", &self.name)
            .field("value", &"[secret]")
            .finish()
    }
}

#[cfg(test)]
#[path = "config_test.rs"]
mod tests;
