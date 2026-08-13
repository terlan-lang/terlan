//! Tests for the package-neutral C++ binding generator.

pub(super) use super::*;
pub(super) use serde_json::Value;
pub(super) use std::collections::BTreeSet;
pub(super) use std::fs;

#[cfg(test)]
#[path = "generator_test/borrowed_resource_test.rs"]
#[cfg(test)]
mod borrowed_resource_test;
#[cfg(test)]
#[path = "generator_test/fixtures_and_generation.rs"]
mod fixtures_and_generation;
use fixtures_and_generation::*;
#[cfg(test)]
#[path = "generator_test/namespace_and_producer.rs"]
mod namespace_and_producer;
#[cfg(test)]
#[path = "generator_test/policy_validation.rs"]
mod policy_validation;
