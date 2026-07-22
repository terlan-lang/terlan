mod imported_type_refs;
mod imports;
mod interface_loading;
mod interface_render;
mod model;
mod naming;
mod shapes;
#[cfg(test)]
#[path = "shapes_test.rs"]
mod shapes_test;
#[cfg(test)]
pub(crate) mod test_support;
include!("mod_part_001.rs");
include!("mod_part_002.rs");
