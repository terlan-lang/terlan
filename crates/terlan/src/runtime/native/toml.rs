//! Rust-native TOML adapter for `std.data.Toml`.

use crate::terlan_native::json::{Json, JsonError};

/// Parses TOML into the portable JSON value representation.
pub fn parse(text: &str) -> Result<Json, JsonError> {
    basic_toml::from_str::<serde_json::Value>(text)
        .map(Json::from_serde)
        .map_err(|error| JsonError::new("toml.parse", error.to_string(), 0))
}

#[cfg(test)]
#[path = "toml_test.rs"]
mod toml_test;
