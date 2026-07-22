mod args;
mod js;
mod js_assets;
mod js_browser;
mod js_model;
mod js_source_classification;
mod metadata;
mod mobile;
mod package_artifact;
mod package_git;
mod package_layout;
mod project_roots;
mod source_roots;
#[cfg(test)]
#[path = "source_roots_test.rs"]
mod source_roots_test;
mod target_gate;
pub(crate) mod vm_artifact;
mod vm_launcher;
pub(crate) mod wasm_artifact;
mod wasm_model;

pub(crate) mod project_manifest;

#[cfg(test)]
mod build_test;
include!("mod_part_001.rs");
include!("mod_part_002.rs");
