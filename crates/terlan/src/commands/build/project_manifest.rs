use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};

use accelerator::ProjectAcceleratorBuilder;

use config::{
    parse_bool, parse_non_negative_u64, parse_server_profile, parse_server_tls_mode,
    parse_server_tls_provider, ProjectServerTlsBuilder, ProjectWebAssetsBuilder,
};
use inline_table::{
    expect_inline_string_array_field, expect_inline_string_field, parse_inline_table,
    ProjectManifestInlineValue,
};
pub(crate) use model::{
    ProjectArtifactKind, ProjectDependency, ProjectDependencyScope, ProjectDependencySource,
    ProjectManifest, ProjectPackage, ProjectScript, ProjectServerProfile, ProjectServerTls,
    ProjectServerTlsMode, ProjectServerTlsProvider, ProjectTarget, ProjectWasiProfile,
    ProjectWasmProfile,
};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use model::{ProjectNativeRust, ProjectWebAssets};
use native_rust::finish_native_rust;
use strings::{parse_string, parse_string_array};
use targets::{
    finish_wasi_target, finish_wasm_target, parse_wasi_profile, parse_wasm_profile,
    ParsedWasmTarget,
};
use validation::{
    validate_dependency_alias, validate_package_name, validate_package_namespace,
    validate_package_version,
};

mod accelerator;
mod config;
mod dependencies;
mod inline_table;
mod model;
mod native_rust;
mod parser;
mod strings;
mod targets;
mod validation;
#[cfg(all(feature = "serve-runtime-bin", not(test)))]
pub(crate) use config::read_runtime_server_tls;
#[cfg(test)]
mod vm_tls;
#[cfg(test)]
pub(crate) use vm_tls::vm_tls_plan_from_project_tls;
#[cfg(test)]
#[path = "project_manifest/vm_tls_test.rs"]
#[cfg(test)]
mod vm_tls_test;

#[cfg(test)]
#[path = "project_manifest_test.rs"]
#[cfg(test)]
mod project_manifest_test;

use dependencies::*;
#[cfg(test)]
pub(crate) use parser::parse_project_manifest;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use parser::read_project_manifest;
