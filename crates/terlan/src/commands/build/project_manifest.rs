mod config;
mod inline_table;
mod model;
mod native_rust;
mod strings;
mod targets;
mod validation;
mod vm_tls;
#[cfg(test)]
#[path = "project_manifest/vm_tls_test.rs"]
mod vm_tls_test;

#[cfg(test)]
#[path = "project_manifest_test.rs"]
mod project_manifest_test;
include!("project_manifest_part_001.rs");
include!("project_manifest_part_002.rs");
