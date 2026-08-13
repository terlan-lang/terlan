pub(super) use super::*;
pub(super) use std::fs;
pub(super) use std::path::PathBuf;
pub(super) use std::process::ExitCode;

pub(super) use crate::terlan_syntax::SyntaxTypeOutput;
pub(super) use crate::terlan_typeck::CoreModule;
pub(super) use crate::validation::native_policy::NativePolicy;
pub(super) use crate::validation::target_profile::TargetProfile;
pub(super) use crate::{CliCommand, CliState};

pub(super) use super::discovery::is_supported_test_return_type;
pub(super) use super::execution::{
    effective_js_test_profile, parse_test_args, remove_compiler_intrinsic_functions, TestTarget,
};
pub(super) use super::manifest::validation_pass_report;
pub(super) use super::project_context::read_vm_test_project_manifest;

#[cfg(test)]
#[path = "test_command_test/configuration.rs"]
mod configuration_cases;
#[cfg(test)]
#[path = "test_command_test/execution.rs"]
mod execution_cases;
