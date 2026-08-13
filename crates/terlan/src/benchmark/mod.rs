//! Internal executable benchmark harnesses for compiler and runtime migration.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::runtime::native::http;
use crate::runtime::native::json;
use crate::runtime::native::postgres::{self, Config, Pool, PostgresError};
use serde::Serialize;

mod http_runtime_lane;

mod aot_compilation;
mod binary_protocol;
mod hardware;
mod http_aot_performance;
mod managed_heap;
mod persistent_actor;
mod runtime_workloads;
mod vm_runtime;

pub(crate) use vm_runtime::{
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
mod http_and_build;
mod map_workloads;
mod measurement;

use cli::*;
use database::*;
use http_and_build::*;
use map_workloads::*;
use measurement::*;

/// Runs the benchmark command selected by process arguments.
pub fn run_from_env() -> ExitCode {
    cli::run()
}
