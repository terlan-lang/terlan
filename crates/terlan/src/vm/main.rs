#![deny(unsafe_code)]
#![allow(dead_code, unused_imports)]

macro_rules! vm_capability_component {
    ($($item:item)*) => {
        $($item)*
    };
}

macro_rules! vm_map_profile_component {
    ($($item:item)*) => {
        $($item)*
    };
}

macro_rules! vm_code_server_test_component {
    ($($item:item)*) => {
        $(#[cfg(test)] $item)*
    };
}

#[path = "commands.rs"]
pub mod commands;
#[path = "../compiler/mod.rs"]
pub mod compiler;
#[path = "../database_schema.rs"]
pub(crate) mod database_schema;
#[path = "../formal_pipeline.rs"]
pub mod formal_pipeline;
#[path = "main/framing_benchmark.rs"]
mod framing_benchmark;
#[path = "../html/mod.rs"]
pub mod html;
#[path = "main/http_attribution.rs"]
mod http_attribution;
#[path = "main/inspection.rs"]
mod inspection;
mod instrumentation;
mod instrumentation_tui;
#[path = "../mobile/mod.rs"]
pub mod mobile;
#[path = "main/native_image_runner.rs"]
mod native_image_runner;
#[path = "../runtime/mod.rs"]
pub mod runtime;
#[path = "../support/mod.rs"]
pub mod support;
#[path = "../validation/mod.rs"]
pub mod validation;

#[cfg(test)]
#[path = "main_test.rs"]
mod main_test;
include!("main_part_001.rs");
include!("main_part_002.rs");
include!("main_part_004.rs");
