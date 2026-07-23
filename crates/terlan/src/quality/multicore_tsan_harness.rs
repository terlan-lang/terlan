#![deny(unsafe_code)]
#![allow(dead_code)]

#[path = "../runtime/vm/actor_directory.rs"]
mod actor_directory;
#[path = "../runtime/vm/fixed_scheduler_control.rs"]
mod fixed_scheduler_control;
#[path = "../runtime/vm/process/identity.rs"]
mod process;
#[path = "../runtime/vm/scheduler_topology.rs"]
mod scheduler_topology;

/// Provides a test executable containing only the production multicore
/// ownership modules admitted by the stable Rust ThreadSanitizer target.
fn main() {}
