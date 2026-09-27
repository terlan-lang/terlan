#![forbid(unsafe_code)]
// AUTO-GENERATED NativeBoundary skeleton.
// Implement concrete native exports only after preserving this bridge contract.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

pub const SOURCE_MODULE: &str = "std.vm.Fault";
pub const NATIVE_MODULE: &str = "std_vm_fault_native_boundary";
pub const SCHEDULER: &str = "normal";

pub const FUNCTIONS: &[(&str, usize)] = &[
    ("policy", 3),
    ("resolve_policy", 2),
    ("compatibility", 2),
    ("monitor", 2),
    ("record_heartbeat", 3),
    ("suspect", 4),
    ("degrade", 4),
    ("isolate", 4),
    ("begin_recovery", 4),
    ("complete", 4),
    ("expire", 4),
    ("state_name", 2),
    ("transitions_after", 2),
    ("failures_after", 2),
    ("close", 1),
    ("transition_node_id", 1),
    ("transition_previous_state", 1),
    ("transition_next_state", 1),
    ("transition_tick", 1),
    ("transition_reason", 1),
    ("transition_diagnostic_kind", 1),
    ("failure_kind", 1),
    ("failure_node_id", 1),
    ("failure_tick", 1),
    ("failure_reason", 1),
    ("failure_diagnostic_kind", 1),
    ("state", 2),
    ("classify_heartbeat", 5),
    ("isolate_partition", 5),
    ("start_recovery", 3),
    ("complete_recovery", 3),
    ("expire_recovery", 4),
    ("migration_timeout", 7),
    ("migration_partial_commit", 7),
    ("failure", 1),
    ("stale_placement_update", 6),
];

pub const OPERATIONS: &[(&str, &str, usize)] = &[
    ("policy", "std.vm.fault.policy", 3),
    ("resolve_policy", "std.vm.fault.resolve_policy", 2),
    ("compatibility", "std.vm.fault.compatibility", 2),
    ("monitor", "std.vm.fault.monitor", 2),
    ("record_heartbeat", "std.vm.fault.record_heartbeat", 3),
    ("suspect", "std.vm.fault.suspect", 4),
    ("degrade", "std.vm.fault.degrade", 4),
    ("isolate", "std.vm.fault.isolate", 4),
    ("begin_recovery", "std.vm.fault.begin_recovery", 4),
    ("complete", "std.vm.fault.complete", 4),
    ("expire", "std.vm.fault.expire", 4),
    ("state_name", "std.vm.fault.state_name", 2),
    ("transitions_after", "std.vm.fault.transitions_after", 2),
    ("failures_after", "std.vm.fault.failures_after", 2),
    ("close", "std.vm.fault.close", 1),
    ("transition_node_id", "std.vm.fault.transition_node_id", 1),
    ("transition_previous_state", "std.vm.fault.transition_previous_state", 1),
    ("transition_next_state", "std.vm.fault.transition_next_state", 1),
    ("transition_tick", "std.vm.fault.transition_tick", 1),
    ("transition_reason", "std.vm.fault.transition_reason", 1),
    ("transition_diagnostic_kind", "std.vm.fault.transition_diagnostic_kind", 1),
    ("failure_kind", "std.vm.fault.failure_kind", 1),
    ("failure_node_id", "std.vm.fault.failure_node_id", 1),
    ("failure_tick", "std.vm.fault.failure_tick", 1),
    ("failure_reason", "std.vm.fault.failure_reason", 1),
    ("failure_diagnostic_kind", "std.vm.fault.failure_diagnostic_kind", 1),
    ("state", "std.vm.fault.state", 2),
    ("classify_heartbeat", "std.vm.fault.classify_heartbeat", 5),
    ("isolate_partition", "std.vm.fault.isolate_partition", 5),
    ("start_recovery", "std.vm.fault.start_recovery", 3),
    ("complete_recovery", "std.vm.fault.complete_recovery", 3),
    ("expire_recovery", "std.vm.fault.expire_recovery", 4),
    ("migration_timeout", "std.vm.fault.migration_timeout", 7),
    ("migration_partial_commit", "std.vm.fault.migration_partial_commit", 7),
    ("failure", "std.vm.fault.failure", 1),
    ("stale_placement_update", "std.vm.fault.stale_placement_update", 6),
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
                        "std.vm.fault.policy" => native_unimplemented_operation(operation),
                        "std.vm.fault.resolve_policy" => native_unimplemented_operation(operation),
                        "std.vm.fault.compatibility" => native_unimplemented_operation(operation),
                        "std.vm.fault.monitor" => native_unimplemented_operation(operation),
                        "std.vm.fault.record_heartbeat" => native_unimplemented_operation(operation),
                        "std.vm.fault.suspect" => native_unimplemented_operation(operation),
                        "std.vm.fault.degrade" => native_unimplemented_operation(operation),
                        "std.vm.fault.isolate" => native_unimplemented_operation(operation),
                        "std.vm.fault.begin_recovery" => native_unimplemented_operation(operation),
                        "std.vm.fault.complete" => native_unimplemented_operation(operation),
                        "std.vm.fault.expire" => native_unimplemented_operation(operation),
                        "std.vm.fault.state_name" => native_unimplemented_operation(operation),
                        "std.vm.fault.transitions_after" => native_unimplemented_operation(operation),
                        "std.vm.fault.failures_after" => native_unimplemented_operation(operation),
                        "std.vm.fault.close" => native_unimplemented_operation(operation),
                        "std.vm.fault.transition_node_id" => native_unimplemented_operation(operation),
                        "std.vm.fault.transition_previous_state" => native_unimplemented_operation(operation),
                        "std.vm.fault.transition_next_state" => native_unimplemented_operation(operation),
                        "std.vm.fault.transition_tick" => native_unimplemented_operation(operation),
                        "std.vm.fault.transition_reason" => native_unimplemented_operation(operation),
                        "std.vm.fault.transition_diagnostic_kind" => native_unimplemented_operation(operation),
                        "std.vm.fault.failure_kind" => native_unimplemented_operation(operation),
                        "std.vm.fault.failure_node_id" => native_unimplemented_operation(operation),
                        "std.vm.fault.failure_tick" => native_unimplemented_operation(operation),
                        "std.vm.fault.failure_reason" => native_unimplemented_operation(operation),
                        "std.vm.fault.failure_diagnostic_kind" => native_unimplemented_operation(operation),
                        "std.vm.fault.state" => native_unimplemented_operation(operation),
                        "std.vm.fault.classify_heartbeat" => native_unimplemented_operation(operation),
                        "std.vm.fault.isolate_partition" => native_unimplemented_operation(operation),
                        "std.vm.fault.start_recovery" => native_unimplemented_operation(operation),
                        "std.vm.fault.complete_recovery" => native_unimplemented_operation(operation),
                        "std.vm.fault.expire_recovery" => native_unimplemented_operation(operation),
                        "std.vm.fault.migration_timeout" => native_unimplemented_operation(operation),
                        "std.vm.fault.migration_partial_commit" => native_unimplemented_operation(operation),
                        "std.vm.fault.failure" => native_unimplemented_operation(operation),
                        "std.vm.fault.stale_placement_update" => native_unimplemented_operation(operation),
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
