#![forbid(unsafe_code)]
// AUTO-GENERATED NativeBoundary skeleton.
// Implement concrete native exports only after preserving this bridge contract.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

pub const SOURCE_MODULE: &str = "std.vm.Cluster";
pub const NATIVE_MODULE: &str = "std_vm_cluster_native_boundary";
pub const SCHEDULER: &str = "normal";

pub const FUNCTIONS: &[(&str, usize)] = &[
    ("profile", 6),
    ("node_id", 1),
    ("epoch", 1),
    ("next_epoch", 1),
    ("membership", 2),
    ("join", 4),
    ("restart", 3),
    ("heartbeat", 3),
    ("partition", 3),
    ("heal", 3),
    ("leave", 3),
    ("fence", 3),
    ("expire", 2),
    ("prune", 3),
    ("state", 2),
    ("view", 1),
    ("health", 1),
    ("leader_hint", 1),
    ("open", 3),
    ("send", 3),
    ("send_with", 4),
    ("accept", 2),
    ("message_id", 1),
    ("delivery", 1),
    ("needs_ack", 2),
    ("pending_ack_count", 1),
    ("acknowledge", 2),
    ("state", 1),
    ("disconnect", 3),
    ("reconnect", 3),
];

pub const OPERATIONS: &[(&str, &str, usize)] = &[
    ("profile", "std.vm.cluster.profile", 6),
    ("node_id", "std.vm.cluster.profile_node_id", 1),
    ("epoch", "std.vm.cluster.profile_epoch", 1),
    ("next_epoch", "std.vm.cluster.profile_next_epoch", 1),
    ("membership", "std.vm.cluster.membership", 2),
    ("join", "std.vm.cluster.membership_join", 4),
    ("restart", "std.vm.cluster.membership_restart", 3),
    ("heartbeat", "std.vm.cluster.membership_heartbeat", 3),
    ("partition", "std.vm.cluster.membership_partition", 3),
    ("heal", "std.vm.cluster.membership_heal", 3),
    ("leave", "std.vm.cluster.membership_leave", 3),
    ("fence", "std.vm.cluster.membership_fence", 3),
    ("expire", "std.vm.cluster.membership_expire", 2),
    ("prune", "std.vm.cluster.membership_prune", 3),
    ("state", "std.vm.cluster.membership_state", 2),
    ("view", "std.vm.cluster.membership_view", 1),
    ("health", "std.vm.cluster.membership_health", 1),
    ("leader_hint", "std.vm.cluster.membership_leader_hint", 1),
    ("open", "std.vm.cluster.open", 3),
    ("send", "std.vm.cluster.send", 3),
    ("send_with", "std.vm.cluster.send_with", 4),
    ("accept", "std.vm.cluster.accept", 2),
    ("message_id", "std.vm.cluster.frame_message_id", 1),
    ("delivery", "std.vm.cluster.frame_delivery", 1),
    ("needs_ack", "std.vm.cluster.needs_ack", 2),
    ("pending_ack_count", "std.vm.cluster.pending_ack_count", 1),
    ("acknowledge", "std.vm.cluster.acknowledge", 2),
    ("state", "std.vm.cluster.session_state", 1),
    ("disconnect", "std.vm.cluster.disconnect", 3),
    ("reconnect", "std.vm.cluster.reconnect", 3),
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
                        "std.vm.cluster.profile" => native_unimplemented_operation(operation),
                        "std.vm.cluster.profile_node_id" => native_unimplemented_operation(operation),
                        "std.vm.cluster.profile_epoch" => native_unimplemented_operation(operation),
                        "std.vm.cluster.profile_next_epoch" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership_join" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership_restart" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership_heartbeat" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership_partition" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership_heal" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership_leave" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership_fence" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership_expire" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership_prune" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership_state" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership_view" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership_health" => native_unimplemented_operation(operation),
                        "std.vm.cluster.membership_leader_hint" => native_unimplemented_operation(operation),
                        "std.vm.cluster.open" => native_unimplemented_operation(operation),
                        "std.vm.cluster.send" => native_unimplemented_operation(operation),
                        "std.vm.cluster.send_with" => native_unimplemented_operation(operation),
                        "std.vm.cluster.accept" => native_unimplemented_operation(operation),
                        "std.vm.cluster.frame_message_id" => native_unimplemented_operation(operation),
                        "std.vm.cluster.frame_delivery" => native_unimplemented_operation(operation),
                        "std.vm.cluster.needs_ack" => native_unimplemented_operation(operation),
                        "std.vm.cluster.pending_ack_count" => native_unimplemented_operation(operation),
                        "std.vm.cluster.acknowledge" => native_unimplemented_operation(operation),
                        "std.vm.cluster.session_state" => native_unimplemented_operation(operation),
                        "std.vm.cluster.disconnect" => native_unimplemented_operation(operation),
                        "std.vm.cluster.reconnect" => native_unimplemented_operation(operation),
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
