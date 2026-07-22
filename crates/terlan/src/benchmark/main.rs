//! Internal executable benchmark harnesses for compiler and runtime migration.

#![forbid(unsafe_code)]

macro_rules! vm_capability_component {
    ($($item:item)*) => {
        $(#[cfg(test)] $item)*
    };
}

macro_rules! vm_map_profile_component {
    ($($item:item)*) => {
        $($item)*
    };
}

#[allow(dead_code)]
#[path = "../database_schema.rs"]
pub(crate) mod database_schema;
#[allow(unused_imports)]
#[path = "native_modules.rs"]
pub(crate) mod terlan_native;
#[allow(dead_code, unused_imports)]
#[path = "../runtime/native_boundary/mod.rs"]
pub(crate) mod terlan_native_boundary;

#[cfg(test)]
mod http_runtime_lane;

mod value;

#[allow(dead_code)]
#[path = "../runtime/native_image/boundary_type.rs"]
mod boundary_type;

mod aot_compilation;
mod binary_protocol;
mod hardware;
mod http_aot_performance;
mod managed_heap;
mod persistent_actor;
mod runtime_workloads;
mod vm_runtime;
include!("main_part_001.rs");
include!("main_part_002.rs");
include!("main_part_003.rs");
include!("main_part_004.rs");
include!("main_part_005.rs");
