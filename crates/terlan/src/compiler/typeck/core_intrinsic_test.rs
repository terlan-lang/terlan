pub(super) use crate::terlan_syntax::parse_module_as_syntax_output;
pub(super) use crate::terlan_typeck::core_intrinsic_lowering::core_primitive_intrinsic;

#[cfg(test)]
#[path = "core_intrinsic_test/surface_intrinsics.rs"]
mod surface_intrinsics;
#[cfg(test)]
#[path = "core_intrinsic_test/vm_primitive_registry.rs"]
mod vm_primitive_registry;
pub(super) use super::*;
