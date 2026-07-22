mod discovery;
mod manifest;
mod style;
#[cfg(test)]
mod test_shape_import_test;
mod vm_runner;
mod wasm;

#[cfg(test)]
#[path = "test_command_test.rs"]
mod test_command_test;
include!("mod_part_001.rs");
include!("mod_part_002.rs");
