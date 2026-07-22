#![forbid(unsafe_code)]
// AUTO-GENERATED NativeBoundary skeleton.
// Implement concrete native exports only after preserving this bridge contract.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

pub const SOURCE_MODULE: &str = "terlan_libpq.Driver";
pub const NATIVE_MODULE: &str = "terlan_libpq_driver_native_boundary";
pub const SCHEDULER: &str = "normal";

pub const FUNCTIONS: &[(&str, usize)] = &[
    ("start", 1),
    ("dispose_connection", 1),
    ("socket", 1),
    ("poll_connect", 1),
    ("error_length", 1),
    ("error_bytes", 1),
    ("clear_parameters", 1),
    ("push_null", 1),
    ("push_text", 2),
    ("send_query", 2),
    ("send_batch", 2),
    ("consume_input", 1),
    ("is_busy", 1),
    ("next_result", 1),
    ("abort", 1),
    ("dispose_result", 1),
    ("status", 1),
    ("row_count", 1),
    ("column_count", 1),
    ("select_column_name", 2),
    ("column_oid", 2),
    ("select_value", 3),
    ("value_length", 1),
    ("value_bytes", 1),
    ("value_is_null", 1),
    ("affected_rows", 1),
];

pub const OPERATIONS: &[(&str, &str, usize)] = &[
    ("start", "postgres.libpq.connection.start", 1),
    ("dispose_connection", "postgres.libpq.connection.dispose", 1),
    ("socket", "postgres.libpq.connection.socket", 1),
    ("poll_connect", "postgres.libpq.connection.poll_connect", 1),
    ("error_length", "postgres.libpq.connection.error_length", 1),
    ("error_bytes", "postgres.libpq.connection.error_bytes", 1),
    ("clear_parameters", "postgres.libpq.connection.clear_parameters", 1),
    ("push_null", "postgres.libpq.connection.push_null", 1),
    ("push_text", "postgres.libpq.connection.push_text", 2),
    ("send_query", "postgres.libpq.connection.send_query", 2),
    ("send_batch", "postgres.libpq.connection.send_batch", 2),
    ("consume_input", "postgres.libpq.connection.consume_input", 1),
    ("is_busy", "postgres.libpq.connection.is_busy", 1),
    ("next_result", "postgres.libpq.connection.next_result", 1),
    ("abort", "postgres.libpq.connection.abort", 1),
    ("dispose_result", "postgres.libpq.result.dispose", 1),
    ("status", "postgres.libpq.result.status", 1),
    ("row_count", "postgres.libpq.result.row_count", 1),
    ("column_count", "postgres.libpq.result.column_count", 1),
    ("select_column_name", "postgres.libpq.result.select_column_name", 2),
    ("column_oid", "postgres.libpq.result.column_oid", 2),
    ("select_value", "postgres.libpq.result.select_value", 3),
    ("value_length", "postgres.libpq.result.value_length", 1),
    ("value_bytes", "postgres.libpq.result.value_bytes", 1),
    ("value_is_null", "postgres.libpq.result.value_is_null", 1),
    ("affected_rows", "postgres.libpq.result.affected_rows", 1),
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
                        "postgres.libpq.connection.start" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.dispose" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.socket" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.poll_connect" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.error_length" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.error_bytes" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.clear_parameters" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.push_null" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.push_text" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.send_query" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.send_batch" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.consume_input" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.is_busy" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.next_result" => native_unimplemented_operation(operation),
                        "postgres.libpq.connection.abort" => native_unimplemented_operation(operation),
                        "postgres.libpq.result.dispose" => native_unimplemented_operation(operation),
                        "postgres.libpq.result.status" => native_unimplemented_operation(operation),
                        "postgres.libpq.result.row_count" => native_unimplemented_operation(operation),
                        "postgres.libpq.result.column_count" => native_unimplemented_operation(operation),
                        "postgres.libpq.result.select_column_name" => native_unimplemented_operation(operation),
                        "postgres.libpq.result.column_oid" => native_unimplemented_operation(operation),
                        "postgres.libpq.result.select_value" => native_unimplemented_operation(operation),
                        "postgres.libpq.result.value_length" => native_unimplemented_operation(operation),
                        "postgres.libpq.result.value_bytes" => native_unimplemented_operation(operation),
                        "postgres.libpq.result.value_is_null" => native_unimplemented_operation(operation),
                        "postgres.libpq.result.affected_rows" => native_unimplemented_operation(operation),
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
