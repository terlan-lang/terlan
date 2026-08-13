mod generator;

#[cfg(test)]
#[path = "c_abi_binding_generator_test.rs"]
#[cfg(test)]
mod c_abi_binding_generator_test;

pub(in crate::commands::bind) use generator::generate_c_abi_bindings;
#[cfg(test)]
use generator::CAbiBindingGenerationSummary;
