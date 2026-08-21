#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use std::fs;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use std::path::{Path, PathBuf};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use std::process::{Command, ExitCode};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use std::time::Instant;

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use crate::commands::artifacts::fingerprint;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use crate::validation::target_profile::{
    explicit_target_profile_override_error, infer_target_profile_from_typed_evidence,
    TargetInferenceInput, TargetProfile,
};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use crate::{CliCommand, CliState};

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use args::{parse_build_args, BuildArgs, BuildTarget};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use metadata::{
    build_package_metadata_with_artifacts, BuildPackageExecutable, ProjectNativeRustDependency,
    ProjectSourceRoot,
};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use package_layout::{source_package_path, validate_project_source_package_root};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use project_roots::{reject_unsupported_external_dependencies, resolve_project_build_roots};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use target_gate::reject_unsupported_target_std_source;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use vm_launcher::{terlan_vm_runner_name, vm_image_has_main_entrypoint};

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod args;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod build_orchestration;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod js;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod js_assets;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod js_browser;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod js_model;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod js_source_classification;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod metadata;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_artifact;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_git;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_layout;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_publish;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_publish_live;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_registry_audit;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_registry_commands;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_registry_error;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_registry_mirror;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_registry_resolver;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_registry_solver;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_registry_transport;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_registry_trust;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod package_registry_yank;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod project_roots;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod release_bundle;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod source_roots;
#[cfg(test)]
#[path = "source_roots_test.rs"]
#[cfg(test)]
mod source_roots_test;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod target_gate;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) mod vm_artifact;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod vm_launcher;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) mod wasm_artifact;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod wasm_model;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) mod web_toolchain;

pub(crate) mod project_manifest;

#[cfg(test)]
mod build_test;

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use build_orchestration::*;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use build_orchestration::{resolve_project_test_dependencies, run, run_package_command};
