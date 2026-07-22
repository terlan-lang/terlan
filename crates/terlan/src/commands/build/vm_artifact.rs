mod checked_cache;
#[cfg(test)]
#[path = "vm_artifact/checked_cache_test.rs"]
mod checked_cache_test;
mod compile;
mod native_cache;
#[cfg(test)]
#[path = "vm_artifact/native_cache_test.rs"]
mod native_cache_test;
pub(crate) mod native_debug;
mod native_descriptor;
#[cfg(test)]
#[path = "vm_artifact/native_descriptor_test.rs"]
mod native_descriptor_test;
pub(crate) mod native_image;
mod native_reuse;
mod native_units;
mod orchestration;
mod parallel_compile;
#[cfg(test)]
#[path = "vm_artifact/parallel_compile_test.rs"]
mod parallel_compile_test;

pub(super) use orchestration::{build_one_vm_artifact, build_vm_application_artifacts};
