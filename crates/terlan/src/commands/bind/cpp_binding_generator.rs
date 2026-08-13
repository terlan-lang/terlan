//! Structured C++ metadata to generated `cxx` package bindings.

mod generator;

#[cfg(test)]
#[path = "cpp_binding_extracted_metadata_test.rs"]
#[cfg(test)]
mod cpp_binding_extracted_metadata_test;
#[cfg(test)]
mod exception_adapter_test;
#[cfg(test)]
mod execution_test;

pub(in crate::commands::bind) use generator::generate_cpp_bindings;
