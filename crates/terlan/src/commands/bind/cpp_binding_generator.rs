//! Structured C++ metadata to generated `cxx` package bindings.

mod enum_adapter;
mod exception_adapter;
mod native_helper;

#[cfg(test)]
#[path = "cpp_binding_extracted_metadata_test.rs"]
mod cpp_binding_extracted_metadata_test;
#[cfg(test)]
#[path = "cpp_binding_generator_test.rs"]
mod cpp_binding_generator_test;
#[cfg(test)]
mod exception_adapter_test;
#[cfg(test)]
mod execution_test;
include!("cpp_binding_generator_part_001.rs");
include!("cpp_binding_generator_part_002.rs");
include!("cpp_binding_generator_part_003.rs");
include!("cpp_binding_generator_part_004.rs");
include!("cpp_binding_generator_part_005.rs");
