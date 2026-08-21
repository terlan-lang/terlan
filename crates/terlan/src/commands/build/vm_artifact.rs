mod checked_cache;
#[cfg(test)]
#[path = "vm_artifact/checked_cache_test.rs"]
#[cfg(test)]
mod checked_cache_test;
mod compile;
mod native_cache;
#[cfg(test)]
#[path = "vm_artifact/native_cache_test.rs"]
#[cfg(test)]
mod native_cache_test;
pub(crate) mod native_debug;
mod native_descriptor;
#[cfg(test)]
#[path = "vm_artifact/native_descriptor_test.rs"]
#[cfg(test)]
mod native_descriptor_test;
pub(crate) mod native_image;
mod native_reuse;
mod native_units;
mod orchestration;
mod output_cleanup;
#[cfg(test)]
#[path = "vm_artifact/output_cleanup_test.rs"]
#[cfg(test)]
mod output_cleanup_test;
mod parallel_compile;
#[cfg(test)]
#[path = "vm_artifact/parallel_compile_test.rs"]
#[cfg(test)]
mod parallel_compile_test;
mod std_source;

#[cfg(any(test, not(feature = "serve-runtime-bin")))]
pub(crate) use orchestration::compile_serve_application;
pub(super) use orchestration::{
    build_one_vm_artifact, build_vm_application_artifacts,
    build_vm_application_artifacts_with_entry,
};
