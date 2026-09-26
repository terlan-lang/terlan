//! Internal executable benchmark harnesses for compiler and runtime migration.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::runtime::native::json;
use crate::runtime::native::postgres::{self, Config, Pool, PostgresError};
use serde::Serialize;

mod binary_protocol;
mod hardware;
mod http_aot_performance;
mod managed_heap;
mod persistent_actor;
mod runtime_workloads;

pub(crate) use crate::runtime::vm::{
    actor, map_value, process, resource, scheduler, table, timer, ReplValue,
};

use actor::{VmActorReceive, VmActorRuntime};
use process::{VmProcessSource, VmProcessTable};
use resource::{VmResourceDescriptor, VmResourceEvent, VmResourceTable, VmResourceTransferPolicy};
use scheduler::{VmScheduler, VmSchedulerDecision, VmSchedulerOutcome};
use table::{VmTableAccess, VmTableStore};
use timer::{VmTimerKind, VmTimerTable};
use ReplValue as VmPrimitiveValue;

mod cli;
mod database;
mod map_workloads;
mod measurement;
mod vm_artifact;

use cli::*;
use database::*;
use map_workloads::*;
use measurement::*;
use vm_artifact::*;

/// Runs the benchmark command selected by process arguments.
pub fn run_from_env() -> ExitCode {
    cli::run()
}
