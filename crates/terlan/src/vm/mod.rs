//! Standalone VM command surface backed by the library-owned runtime.
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "main/framing_benchmark.rs"]
mod framing_benchmark;
#[path = "main/http_attribution.rs"]
#[cfg(test)]
mod http_attribution;
#[path = "main/inspection.rs"]
mod inspection;
mod instrumentation;
#[cfg(test)]
mod instrumentation_tui;
#[path = "main/native_image_runner.rs"]
mod native_image_runner;
use crate::runtime;

use runtime::native_image::package_validation::{
    describe_packaged_tvm_image, validate_and_execute_release_package,
};
#[cfg(test)]
use runtime::vm::http_metrics::VmHttpQueueMetrics;
use runtime::vm::persistent_actor_restore::{
    build_cross_machine_actor_export, generate_minimal_actor_replay_fixture,
    plan_persistent_actor_restore, VmPersistentActorExport, VmPersistentActorRestoreCapabilities,
    VmPersistentActorRestoreTarget,
};
use runtime::vm::persistent_actor_store::{
    VmPersistentActorDurability, VmPersistentActorEvent, VmPersistentActorId,
    VmPersistentActorSchema, VmPersistentActorSnapshot,
};
use runtime::vm::pure_native::PureNativeExecutionShard;
use runtime::vm::ReplValue;

use framing_benchmark::{benchmark_in_memory_framing, BenchmarkFramingWorkload};
use inspection::inspect_local_vm;
use native_image_runner::{is_tvm_image_path, render_tvm_support_bundle, run_tvm_image};

mod arguments;
mod cli;
mod execution;

use arguments::*;
use cli::*;
use execution::*;

#[cfg(test)]
#[path = "main_test.rs"]
#[cfg(test)]
mod main_test;

/// Runs the standalone VM command using process arguments.
pub fn run_from_env() -> ExitCode {
    cli::run()
}
