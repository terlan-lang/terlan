//! Standalone VM command surface backed by the library-owned runtime.
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "main/framing_benchmark.rs"]
mod framing_benchmark;
#[path = "main/inspection.rs"]
mod inspection;
#[cfg(test)]
mod instrumentation;
#[cfg(test)]
mod instrumentation_tui;
#[path = "main/native_image_runner.rs"]
mod native_image_runner;
use crate::runtime;

use runtime::native_image::package_validation::{
    describe_packaged_tvm_image, validate_and_execute_release_package,
};
use runtime::vm::package_native_helper::VmStorageBinding;
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
mod embedded;
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
    if embedded::worker_role() {
        return crate::native_worker::run_from_env();
    }
    let application = crate::runtime::executable::EmbeddedApplication::open_current();
    match application {
        Ok(Some(mut application)) => {
            return embedded::run(&mut application).unwrap_or_else(|message| {
                eprintln!("{message}");
                ExitCode::FAILURE
            })
        }
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
        Ok(None) => {}
    }
    if std::env::args_os()
        .skip(1)
        .eq(["--executable-format-version"])
    {
        println!("1");
        return ExitCode::SUCCESS;
    }
    cli::run()
}
