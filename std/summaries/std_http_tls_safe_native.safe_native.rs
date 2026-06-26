#![forbid(unsafe_code)]
// AUTO-GENERATED SafeNative skeleton.
// Implement concrete native exports only after preserving this bridge contract.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

pub const SOURCE_MODULE: &str = "std.http.Tls";
pub const NATIVE_MODULE: &str = "std_http_tls_safe_native";
pub const SCHEDULER: &str = "normal";

pub const FUNCTIONS: &[(&str, usize)] = &[
    ("auto", 2),
    ("manual", 2),
    ("internal", 1),
];

pub const OPERATIONS: &[(&str, &str, usize)] = &[
    ("auto", "std.http.tls.auto", 2),
    ("manual", "std.http.tls.manual", 2),
    ("internal", "std.http.tls.internal", 1),
];

pub const DEFAULT_CREDIT_WINDOW: usize = 32;

// Rust owns native resources. BEAM/Terlan terms should hold only opaque handles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeNativeHandle {
    pub id: u64,
    pub generation: u64,
    pub type_name: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SafeNativeError {
    pub code: &'static str,
    pub message: String,
    pub offset: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SafeNativeValue {
    Unit,
    Text(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Handle(SafeNativeHandle),
    OptionalText(Option<String>),
    OptionalHandle(Option<SafeNativeHandle>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SafeNativeReply {
    pub request_id: u64,
    pub result: Result<SafeNativeValue, SafeNativeError>,
    pub credits: usize,
}

pub struct SafeNativeWorker {
    tx: Sender<SafeNativeCommand>,
    join: Option<JoinHandle<()>>,
    credit_window: usize,
}

enum SafeNativeCommand {
    Register { request_id: u64, type_name: &'static str, reply: Sender<SafeNativeReply> },
    Call { request_id: u64, operation: &'static str, args: Vec<SafeNativeValue>, reply: Sender<SafeNativeReply> },
    Dispose { request_id: u64, handle: SafeNativeHandle, reply: Sender<SafeNativeReply> },
    Stop,
}

impl SafeNativeWorker {
    pub fn start(credit_window: usize) -> Self {
        let credit_window = credit_window.max(1);
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || worker_loop(rx, credit_window));
        Self { tx, join: Some(join), credit_window }
    }

    pub fn credit_window(&self) -> usize {
        self.credit_window
    }

    pub fn register_resource(&self, request_id: u64, type_name: &'static str) -> SafeNativeReply {
        let (reply, rx) = mpsc::channel();
        self.send_and_recv(SafeNativeCommand::Register { request_id, type_name, reply }, request_id, rx)
    }

    pub fn call(&self, request_id: u64, operation: &'static str, args: Vec<SafeNativeValue>) -> SafeNativeReply {
        let (reply, rx) = mpsc::channel();
        self.send_and_recv(SafeNativeCommand::Call { request_id, operation, args, reply }, request_id, rx)
    }

    pub fn dispose(&self, request_id: u64, handle: SafeNativeHandle) -> SafeNativeReply {
        let (reply, rx) = mpsc::channel();
        self.send_and_recv(SafeNativeCommand::Dispose { request_id, handle, reply }, request_id, rx)
    }

    pub fn request_stop(&self) {
        let _ = self.tx.send(SafeNativeCommand::Stop);
    }

    pub fn stop(mut self) {
        self.request_stop();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    fn send_and_recv(&self, command: SafeNativeCommand, request_id: u64, rx: Receiver<SafeNativeReply>) -> SafeNativeReply {
        if self.tx.send(command).is_err() {
            return native_error_reply(request_id, "native_worker_stopped", "native worker is not accepting requests", 0);
        }
        rx.recv().unwrap_or_else(|_| native_error_reply(request_id, "native_worker_stopped", "native worker stopped before replying", 0))
    }
}

impl Drop for SafeNativeWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(SafeNativeCommand::Stop);
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

fn worker_loop(rx: Receiver<SafeNativeCommand>, credit_window: usize) {
    let mut next_id = 1_u64;
    let mut resources = HashMap::<u64, ResourceState>::new();
    while let Ok(command) = rx.recv() {
        match command {
            SafeNativeCommand::Register { request_id, type_name, reply } => {
                let id = next_id;
                next_id += 1;
                let handle = SafeNativeHandle { id, generation: 1, type_name };
                resources.insert(id, ResourceState { generation: handle.generation, type_name });
                let _ = reply.send(SafeNativeReply { request_id, result: Ok(SafeNativeValue::Handle(handle)), credits: credit_window });
            }
            SafeNativeCommand::Call { request_id, operation, args, reply } => {
                let result = match validate_args(&resources, &args) {
                    Ok(()) => match operation {
                        "std.http.tls.auto" => native_unimplemented_operation(operation),
                        "std.http.tls.manual" => native_unimplemented_operation(operation),
                        "std.http.tls.internal" => native_unimplemented_operation(operation),
                        _ => native_unknown_operation(operation),
                    },
                    Err(err) => Err(err),
                };
                let _ = reply.send(SafeNativeReply { request_id, result, credits: credit_window });
            }
            SafeNativeCommand::Dispose { request_id, handle, reply } => {
                let result = match validate_handle(&resources, &handle) {
                    Ok(()) => {
                        resources.remove(&handle.id);
                        Ok(SafeNativeValue::Unit)
                    }
                    Err(err) => Err(err),
                };
                let _ = reply.send(SafeNativeReply { request_id, result, credits: credit_window });
            }
            SafeNativeCommand::Stop => break,
        }
    }
}

fn native_unimplemented_operation(operation: &'static str) -> Result<SafeNativeValue, SafeNativeError> {
    Err(SafeNativeError { code: "native_operation_unimplemented", message: format!("native operation {} is declared but not implemented", operation), offset: 0 })
}

fn native_unknown_operation(operation: &'static str) -> Result<SafeNativeValue, SafeNativeError> {
    Err(SafeNativeError { code: "native_operation_unknown", message: format!("native operation {} is not declared in this adapter", operation), offset: 0 })
}

fn validate_args(resources: &HashMap<u64, ResourceState>, args: &[SafeNativeValue]) -> Result<(), SafeNativeError> {
    for arg in args {
        validate_value_arg(resources, arg)?;
    }
    Ok(())
}

fn validate_value_arg(resources: &HashMap<u64, ResourceState>, arg: &SafeNativeValue) -> Result<(), SafeNativeError> {
    match arg {
        SafeNativeValue::Handle(handle) => validate_handle(resources, handle),
        SafeNativeValue::OptionalHandle(Some(handle)) => validate_handle(resources, handle),
        _ => Ok(()),
    }
}

fn validate_handle(resources: &HashMap<u64, ResourceState>, handle: &SafeNativeHandle) -> Result<(), SafeNativeError> {
    match resources.get(&handle.id) {
        Some(resource) if resource.generation == handle.generation && resource.type_name == handle.type_name => Ok(()),
        _ => Err(SafeNativeError { code: "stale_native_handle", message: format!("native handle {} generation {} is not live", handle.id, handle.generation), offset: 0 }),
    }
}

fn native_error_reply(request_id: u64, code: &'static str, message: &str, credits: usize) -> SafeNativeReply {
    SafeNativeReply { request_id, result: Err(SafeNativeError { code, message: message.to_string(), offset: 0 }), credits }
}
