pub(super) use super::*;
pub(super) use base64::engine::general_purpose::STANDARD;
pub(super) use base64::Engine;
pub(super) use serde_json::Value;
pub(super) use std::collections::BTreeSet;
pub(super) use std::fs;
pub(super) use std::io::{BufRead, BufReader, Write};
pub(super) use std::path::{Path, PathBuf};
pub(super) use std::process::{ExitCode, Stdio};

#[cfg(test)]
#[path = "c_abi_binding_generator_test/fixtures_and_generation.rs"]
mod fixtures_and_generation;
use fixtures_and_generation::*;
#[cfg(test)]
#[path = "c_abi_binding_generator_test/ownership_adapters.rs"]
mod ownership_adapters;
#[cfg(test)]
#[path = "c_abi_binding_generator_test/validation_and_distribution.rs"]
mod validation_and_distribution;
