#![forbid(unsafe_code)]
// AUTO-GENERATED NativeBoundary skeleton.
// Implement concrete native exports only after preserving this bridge contract.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

pub const SOURCE_MODULE: &str = "std.vm.Scheduler";
pub const NATIVE_MODULE: &str = "std_vm_scheduler_native_boundary";
pub const SCHEDULER: &str = "normal";

pub const FUNCTIONS: &[(&str, usize)] = &[
    ("node", 2),
    ("new", 1),
    ("round_robin", 0),
    ("least_connections", 0),
    ("pinned", 1),
    ("shard_affinity", 2),
    ("update_load", 3),
    ("place", 3),
    ("declare_route_policy", 3),
    ("declare_actor_group_policy", 4),
    ("place_for_route", 4),
    ("place_for_actor_group", 5),
    ("refresh", 2),
    ("request_migration", 5),
    ("advance_migration", 3),
    ("commit_migration", 2),
    ("rollback_migration", 3),
    ("abort_migration", 3),
    ("events_after", 2),
    ("placement_scheduler", 1),
    ("placement", 1),
    ("placement_actor_id", 1),
    ("placement_node_id", 1),
    ("placement_policy", 1),
    ("placement_fallback_used", 1),
    ("migration_scheduler", 1),
    ("migration", 1),
    ("migration_actor_id", 1),
    ("migration_from_node_id", 1),
    ("migration_to_node_id", 1),
    ("migration_sequence", 1),
    ("migration_stateful", 1),
    ("migration_phase", 1),
    ("outcome_scheduler", 1),
    ("outcome", 1),
    ("outcome_kind", 1),
    ("outcome_sequence", 1),
    ("outcome_reason", 1),
];

pub const OPERATIONS: &[(&str, &str, usize)] = &[
    ("node", "std.vm.scheduler.node", 2),
    ("new", "std.vm.scheduler.new", 1),
    ("round_robin", "std.vm.scheduler.round_robin", 0),
    ("least_connections", "std.vm.scheduler.least_connections", 0),
    ("pinned", "std.vm.scheduler.pinned", 1),
    ("shard_affinity", "std.vm.scheduler.shard_affinity", 2),
    ("update_load", "std.vm.scheduler.update_load", 3),
    ("place", "std.vm.scheduler.place", 3),
    ("declare_route_policy", "std.vm.scheduler.declare_route_policy", 3),
    ("declare_actor_group_policy", "std.vm.scheduler.declare_actor_group_policy", 4),
    ("place_for_route", "std.vm.scheduler.place_for_route", 4),
    ("place_for_actor_group", "std.vm.scheduler.place_for_actor_group", 5),
    ("refresh", "std.vm.scheduler.refresh", 2),
    ("request_migration", "std.vm.scheduler.request_migration", 5),
    ("advance_migration", "std.vm.scheduler.advance_migration", 3),
    ("commit_migration", "std.vm.scheduler.commit_migration", 2),
    ("rollback_migration", "std.vm.scheduler.rollback_migration", 3),
    ("abort_migration", "std.vm.scheduler.abort_migration", 3),
    ("events_after", "std.vm.scheduler.events_after", 2),
    ("placement_scheduler", "std.vm.scheduler.placement_scheduler", 1),
    ("placement", "std.vm.scheduler.placement", 1),
    ("placement_actor_id", "std.vm.scheduler.placement_actor_id", 1),
    ("placement_node_id", "std.vm.scheduler.placement_node_id", 1),
    ("placement_policy", "std.vm.scheduler.placement_policy", 1),
    ("placement_fallback_used", "std.vm.scheduler.placement_fallback_used", 1),
    ("migration_scheduler", "std.vm.scheduler.migration_scheduler", 1),
    ("migration", "std.vm.scheduler.migration", 1),
    ("migration_actor_id", "std.vm.scheduler.migration_actor_id", 1),
    ("migration_from_node_id", "std.vm.scheduler.migration_from_node_id", 1),
    ("migration_to_node_id", "std.vm.scheduler.migration_to_node_id", 1),
    ("migration_sequence", "std.vm.scheduler.migration_sequence", 1),
    ("migration_stateful", "std.vm.scheduler.migration_stateful", 1),
    ("migration_phase", "std.vm.scheduler.migration_phase", 1),
    ("outcome_scheduler", "std.vm.scheduler.outcome_scheduler", 1),
    ("outcome", "std.vm.scheduler.outcome", 1),
    ("outcome_kind", "std.vm.scheduler.outcome_kind", 1),
    ("outcome_sequence", "std.vm.scheduler.outcome_sequence", 1),
    ("outcome_reason", "std.vm.scheduler.outcome_reason", 1),
];

pub const DEFAULT_CREDIT_WINDOW: usize = 32;

// Rust owns native resources. VM/Terlan terms should hold only opaque handles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeBoundaryHandle {
    pub id: u64,
    pub generation: u64,
    pub type_name: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeBoundaryError {
    pub code: &'static str,
    pub message: String,
    pub offset: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NativeBoundaryValue {
    Unit,
    Text(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Handle(NativeBoundaryHandle),
    OptionalText(Option<String>),
    OptionalHandle(Option<NativeBoundaryHandle>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativeBoundaryReply {
    pub request_id: u64,
    pub result: Result<NativeBoundaryValue, NativeBoundaryError>,
    pub credits: usize,
}

pub struct NativeBoundaryWorker {
    tx: Sender<NativeBoundaryCommand>,
    join: Option<JoinHandle<()>>,
    credit_window: usize,
}

enum NativeBoundaryCommand {
    Register { request_id: u64, type_name: &'static str, reply: Sender<NativeBoundaryReply> },
    Call { request_id: u64, operation: &'static str, args: Vec<NativeBoundaryValue>, reply: Sender<NativeBoundaryReply> },
    Dispose { request_id: u64, handle: NativeBoundaryHandle, reply: Sender<NativeBoundaryReply> },
    Stop,
}

impl NativeBoundaryWorker {
    pub fn start(credit_window: usize) -> Self {
        let credit_window = credit_window.max(1);
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || worker_loop(rx, credit_window));
        Self { tx, join: Some(join), credit_window }
    }

    pub fn credit_window(&self) -> usize {
        self.credit_window
    }

    pub fn register_resource(&self, request_id: u64, type_name: &'static str) -> NativeBoundaryReply {
        let (reply, rx) = mpsc::channel();
        self.send_and_recv(NativeBoundaryCommand::Register { request_id, type_name, reply }, request_id, rx)
    }

    pub fn call(&self, request_id: u64, operation: &'static str, args: Vec<NativeBoundaryValue>) -> NativeBoundaryReply {
        let (reply, rx) = mpsc::channel();
        self.send_and_recv(NativeBoundaryCommand::Call { request_id, operation, args, reply }, request_id, rx)
    }

    pub fn dispose(&self, request_id: u64, handle: NativeBoundaryHandle) -> NativeBoundaryReply {
        let (reply, rx) = mpsc::channel();
        self.send_and_recv(NativeBoundaryCommand::Dispose { request_id, handle, reply }, request_id, rx)
    }

    pub fn request_stop(&self) {
        let _ = self.tx.send(NativeBoundaryCommand::Stop);
    }

    pub fn stop(mut self) {
        self.request_stop();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    fn send_and_recv(&self, command: NativeBoundaryCommand, request_id: u64, rx: Receiver<NativeBoundaryReply>) -> NativeBoundaryReply {
        if self.tx.send(command).is_err() {
            return native_error_reply(request_id, "native_worker_stopped", "native worker is not accepting requests", 0);
        }
        rx.recv().unwrap_or_else(|_| native_error_reply(request_id, "native_worker_stopped", "native worker stopped before replying", 0))
    }
}

impl Drop for NativeBoundaryWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(NativeBoundaryCommand::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ResourceState {
    generation: u64,
    type_name: &'static str,
}

fn worker_loop(rx: Receiver<NativeBoundaryCommand>, credit_window: usize) {
    let mut next_id = 1_u64;
    let mut resources = HashMap::<u64, ResourceState>::new();
    while let Ok(command) = rx.recv() {
        match command {
            NativeBoundaryCommand::Register { request_id, type_name, reply } => {
                let id = next_id;
                next_id += 1;
                let handle = NativeBoundaryHandle { id, generation: 1, type_name };
                resources.insert(id, ResourceState { generation: handle.generation, type_name });
                let _ = reply.send(NativeBoundaryReply { request_id, result: Ok(NativeBoundaryValue::Handle(handle)), credits: credit_window });
            }
            NativeBoundaryCommand::Call { request_id, operation, args, reply } => {
                let result = match validate_args(&resources, &args) {
                    Ok(()) => match operation {
                        "std.vm.scheduler.node" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.new" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.round_robin" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.least_connections" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.pinned" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.shard_affinity" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.update_load" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.place" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.declare_route_policy" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.declare_actor_group_policy" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.place_for_route" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.place_for_actor_group" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.refresh" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.request_migration" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.advance_migration" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.commit_migration" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.rollback_migration" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.abort_migration" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.events_after" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.placement_scheduler" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.placement" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.placement_actor_id" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.placement_node_id" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.placement_policy" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.placement_fallback_used" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.migration_scheduler" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.migration" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.migration_actor_id" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.migration_from_node_id" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.migration_to_node_id" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.migration_sequence" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.migration_stateful" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.migration_phase" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.outcome_scheduler" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.outcome" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.outcome_kind" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.outcome_sequence" => native_unimplemented_operation(operation),
                        "std.vm.scheduler.outcome_reason" => native_unimplemented_operation(operation),
                        _ => native_unknown_operation(operation),
                    },
                    Err(err) => Err(err),
                };
                let _ = reply.send(NativeBoundaryReply { request_id, result, credits: credit_window });
            }
            NativeBoundaryCommand::Dispose { request_id, handle, reply } => {
                let result = match validate_handle(&resources, &handle) {
                    Ok(()) => {
                        resources.remove(&handle.id);
                        Ok(NativeBoundaryValue::Unit)
                    }
                    Err(err) => Err(err),
                };
                let _ = reply.send(NativeBoundaryReply { request_id, result, credits: credit_window });
            }
            NativeBoundaryCommand::Stop => break,
        }
    }
}

fn native_unimplemented_operation(operation: &'static str) -> Result<NativeBoundaryValue, NativeBoundaryError> {
    Err(NativeBoundaryError { code: "native_operation_unimplemented", message: format!("native operation {} is declared but not implemented", operation), offset: 0 })
}

fn native_unknown_operation(operation: &'static str) -> Result<NativeBoundaryValue, NativeBoundaryError> {
    Err(NativeBoundaryError { code: "native_operation_unknown", message: format!("native operation {} is not declared in this adapter", operation), offset: 0 })
}

fn validate_args(resources: &HashMap<u64, ResourceState>, args: &[NativeBoundaryValue]) -> Result<(), NativeBoundaryError> {
    for arg in args {
        validate_value_arg(resources, arg)?;
    }
    Ok(())
}

fn validate_value_arg(resources: &HashMap<u64, ResourceState>, arg: &NativeBoundaryValue) -> Result<(), NativeBoundaryError> {
    match arg {
        NativeBoundaryValue::Handle(handle) => validate_handle(resources, handle),
        NativeBoundaryValue::OptionalHandle(Some(handle)) => validate_handle(resources, handle),
        _ => Ok(()),
    }
}

fn validate_handle(resources: &HashMap<u64, ResourceState>, handle: &NativeBoundaryHandle) -> Result<(), NativeBoundaryError> {
    match resources.get(&handle.id) {
        Some(resource) if resource.generation == handle.generation && resource.type_name == handle.type_name => Ok(()),
        _ => Err(NativeBoundaryError { code: "stale_native_handle", message: format!("native handle {} generation {} is not live", handle.id, handle.generation), offset: 0 }),
    }
}

fn native_error_reply(request_id: u64, code: &'static str, message: &str, credits: usize) -> NativeBoundaryReply {
    NativeBoundaryReply { request_id, result: Err(NativeBoundaryError { code, message: message.to_string(), offset: 0 }), credits }
}
