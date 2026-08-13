use super::contracts::*;

const POLARS_RUST_BRIDGE: &str = r#"#![forbid(unsafe_code)]
//! Supervised native worker probe for `std.native.polars`.
//!
//! Inputs:
//! - Typed bridge commands from the future VM/native adapter boundary.
//! - Opaque handles created and owned by the native worker.
//!
//! Outputs:
//! - Typed replies carrying request ids, stable errors, and credit information.
//! - Opaque handle values that VM can store without seeing native pointers.
//!
//! Transformation:
//! - Models the Terlan supervised actor bridge without linking Polars yet. The
//!   real native target can replace the probe thread with a VM-owned
//!   NativeBoundary worker while preserving the same command/reply and
//!   handle-generation contract.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

pub const DEFAULT_CREDIT_WINDOW: usize = 32;
pub const DATAFRAME_TYPE: &str = "std.native.polars.DataFrame.DataFrame";

/// Opaque native handle carried by Terlan/VM terms.
///
/// Inputs:
/// - Numeric resource id and generation assigned by the worker.
/// - Stable source-level type name for diagnostics and type checks.
///
/// Outputs:
/// - Copyable handle token with no raw pointer or native storage.
///
/// Transformation:
/// - Separates native ownership from Terlan values while allowing stale-handle
///   detection through generation tokens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeHandle {
    pub id: u64,
    pub generation: u64,
    pub type_name: &'static str,
}

/// Stable native bridge error.
///
/// Inputs:
/// - Static error code and owned message text.
///
/// Outputs:
/// - Error shape suitable for lowering into `std.core.Error.Error`.
///
/// Transformation:
/// - Keeps worker failures target-neutral and independent from Rust panic or
///   transport details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeBridgeError {
    pub code: &'static str,
    pub message: String,
}

impl NativeBridgeError {
    /// Creates a stable bridge error.
    ///
    /// Inputs:
    /// - `code`: stable machine-readable error code.
    /// - `message`: human-readable diagnostic text.
    ///
    /// Outputs:
    /// - `NativeBridgeError` with owned message storage.
    ///
    /// Transformation:
    /// - Normalizes arbitrary message inputs into the bridge error shape.
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Typed value returned by the worker probe.
///
/// Inputs:
/// - Native command execution results.
///
/// Outputs:
/// - Small target-neutral value set used by bridge tests.
///
/// Transformation:
/// - Avoids exposing Rust resources directly while still proving command/reply
///   routing and typed return values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeValue {
    Unit,
    Int(i64),
    Handle(NativeHandle),
}

/// Worker reply carrying request correlation and backpressure state.
///
/// Inputs:
/// - Request id supplied by the caller.
/// - Worker operation result.
/// - Remaining advertised credit window.
///
/// Outputs:
/// - Reply value the VM side can match against the original request.
///
/// Transformation:
/// - Makes request/reply correlation and credit-based flow control explicit in
///   the ABI-level probe.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeReply {
    pub request_id: u64,
    pub result: Result<NativeValue, NativeBridgeError>,
    pub credits: usize,
}

/// Supervised native worker handle.
///
/// Inputs:
/// - Start requests from the VM supervision boundary.
///
/// Outputs:
/// - A command sender plus owned worker join handle.
///
/// Transformation:
/// - Owns native resource state on the Rust side and exposes only typed
///   request methods to callers.
pub struct SupervisedNativeWorker {
    tx: Sender<WorkerCommand>,
    join: Option<JoinHandle<()>>,
    credit_window: usize,
}

/// Commands accepted by the supervised native worker probe.
///
/// Inputs:
/// - VM-side request data, native handles, operation names, and reply
///   channels.
///
/// Outputs:
/// - Worker-loop actions that allocate, call, dispose, or stop native state.
///
/// Transformation:
/// - Serializes mutable native resource access into one Rust-owned command
///   stream.
enum WorkerCommand {
    AllocateDataFrame {
        request_id: u64,
        reply: Sender<NativeReply>,
    },
    Call {
        request_id: u64,
        handle: NativeHandle,
        operation: &'static str,
        reply: Sender<NativeReply>,
    },
    Dispose {
        request_id: u64,
        handle: NativeHandle,
        reply: Sender<NativeReply>,
    },
    Stop,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Native resource entry owned by the worker.
///
/// Inputs:
/// - Allocated native handle metadata.
///
/// Outputs:
/// - Resource-table value used to validate handles.
///
/// Transformation:
/// - Stores generation and type metadata so stale or forged handles can be
///   rejected before operations run.
struct Resource {
    generation: u64,
    type_name: &'static str,
}

impl SupervisedNativeWorker {
    /// Starts a supervised native worker probe.
    ///
    /// Inputs:
    /// - `credit_window`: maximum advertised outstanding request budget.
    ///
    /// Outputs:
    /// - Running worker handle.
    ///
    /// Transformation:
    /// - Spawns a Rust-owned actor loop that serializes mutable resource access.
    pub fn start(credit_window: usize) -> Self {
        let credit_window = credit_window.max(1);
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || worker_loop(rx, credit_window));

        Self {
            tx,
            join: Some(join),
            credit_window,
        }
    }

    /// Returns the worker credit window.
    ///
    /// Inputs:
    /// - `self`: running worker handle.
    ///
    /// Outputs:
    /// - Configured positive credit window.
    ///
    /// Transformation:
    /// - Exposes the backpressure budget recorded in worker replies.
    pub fn credit_window(&self) -> usize {
        self.credit_window
    }

    /// Allocates an opaque DataFrame handle.
    ///
    /// Inputs:
    /// - `request_id`: caller-supplied request correlation id.
    ///
    /// Outputs:
    /// - Reply containing `NativeValue::Handle` or a stable bridge error.
    ///
    /// Transformation:
    /// - Creates worker-owned resource state and returns only a typed handle.
    pub fn allocate_dataframe(&self, request_id: u64) -> NativeReply {
        let (reply, rx) = mpsc::channel();
        self.send_and_recv(
            WorkerCommand::AllocateDataFrame { request_id, reply },
            request_id,
            rx,
        )
    }

    /// Calls a read-only observer operation on a handle.
    ///
    /// Inputs:
    /// - `request_id`: caller-supplied request correlation id.
    /// - `handle`: opaque resource handle previously returned by the worker.
    /// - `operation`: selected operation name.
    ///
    /// Outputs:
    /// - Reply containing a typed result or stable bridge error.
    ///
    /// Transformation:
    /// - Routes operation execution through the resource owner actor and
    ///   validates handle generation before producing a result.
    pub fn call(
        &self,
        request_id: u64,
        handle: NativeHandle,
        operation: &'static str,
    ) -> NativeReply {
        let (reply, rx) = mpsc::channel();
        self.send_and_recv(
            WorkerCommand::Call {
                request_id,
                handle,
                operation,
                reply,
            },
            request_id,
            rx,
        )
    }

    /// Disposes a native resource handle.
    ///
    /// Inputs:
    /// - `request_id`: caller-supplied request correlation id.
    /// - `handle`: opaque resource handle to release.
    ///
    /// Outputs:
    /// - Reply containing `NativeValue::Unit` or a stale-handle error.
    ///
    /// Transformation:
    /// - Releases worker-owned state while preserving generation-token checks.
    pub fn dispose(&self, request_id: u64, handle: NativeHandle) -> NativeReply {
        let (reply, rx) = mpsc::channel();
        self.send_and_recv(
            WorkerCommand::Dispose {
                request_id,
                handle,
                reply,
            },
            request_id,
            rx,
        )
    }

    /// Stops the worker and joins its thread.
    ///
    /// Inputs:
    /// - `self`: owned worker handle.
    ///
    /// Outputs:
    /// - None.
    ///
    /// Transformation:
    /// - Sends an explicit stop command and waits for Rust-side cleanup.
    pub fn stop(mut self) {
        let _ = self.tx.send(WorkerCommand::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }

    /// Sends one command and waits for the correlated reply.
    ///
    /// Inputs:
    /// - `command`: worker command carrying its reply sender.
    /// - `request_id`: request id used if channel delivery fails.
    /// - `rx`: one-shot reply receiver.
    ///
    /// Outputs:
    /// - Worker reply or stable channel failure.
    ///
    /// Transformation:
    /// - Converts transport failures into bridge errors instead of panicking.
    fn send_and_recv(
        &self,
        command: WorkerCommand,
        request_id: u64,
        rx: Receiver<NativeReply>,
    ) -> NativeReply {
        if self.tx.send(command).is_err() {
            return NativeReply {
                request_id,
                result: Err(NativeBridgeError::new(
                    "native_worker_stopped",
                    "native worker is not accepting requests",
                )),
                credits: 0,
            };
        }

        rx.recv().unwrap_or_else(|_| NativeReply {
            request_id,
            result: Err(NativeBridgeError::new(
                "native_worker_stopped",
                "native worker stopped before replying",
            )),
            credits: 0,
        })
    }
}

impl Drop for SupervisedNativeWorker {
    /// Stops the worker when the handle is dropped.
    ///
    /// Inputs:
    /// - `self`: worker handle being dropped.
    ///
    /// Outputs:
    /// - None.
    ///
    /// Transformation:
    /// - Provides cleanup for tests and future VM resource finalizers that do
    ///   not call `stop` explicitly.
    fn drop(&mut self) {
        let _ = self.tx.send(WorkerCommand::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Runs the resource-owner actor loop.
///
/// Inputs:
/// - `rx`: command receiver owned by the worker thread.
/// - `credit_window`: advertised request credit budget.
///
/// Outputs:
/// - None.
///
/// Transformation:
/// - Serializes all mutable resource operations and returns typed replies over
///   command-local reply channels.
fn worker_loop(rx: Receiver<WorkerCommand>, credit_window: usize) {
    let mut next_id = 1_u64;
    let mut resources = HashMap::<u64, Resource>::new();

    while let Ok(command) = rx.recv() {
        match command {
            WorkerCommand::AllocateDataFrame { request_id, reply } => {
                let id = next_id;
                next_id += 1;
                let handle = NativeHandle {
                    id,
                    generation: 1,
                    type_name: DATAFRAME_TYPE,
                };
                resources.insert(
                    id,
                    Resource {
                        generation: handle.generation,
                        type_name: handle.type_name,
                    },
                );
                let _ = reply.send(NativeReply {
                    request_id,
                    result: Ok(NativeValue::Handle(handle)),
                    credits: credit_window,
                });
            }
            WorkerCommand::Call {
                request_id,
                handle,
                operation,
                reply,
            } => {
                let result = match validate_handle(&resources, &handle) {
                    Ok(()) => call_operation(operation),
                    Err(err) => Err(err),
                };
                let _ = reply.send(NativeReply {
                    request_id,
                    result,
                    credits: credit_window,
                });
            }
            WorkerCommand::Dispose {
                request_id,
                handle,
                reply,
            } => {
                let result = match validate_handle(&resources, &handle) {
                    Ok(()) => {
                        resources.remove(&handle.id);
                        Ok(NativeValue::Unit)
                    }
                    Err(err) => Err(err),
                };
                let _ = reply.send(NativeReply {
                    request_id,
                    result,
                    credits: credit_window,
                });
            }
            WorkerCommand::Stop => break,
        }
    }
}

/// Validates an opaque handle against worker-owned resources.
///
/// Inputs:
/// - `resources`: current worker resource table.
/// - `handle`: caller-provided opaque handle.
///
/// Outputs:
/// - `Ok(())` when id, generation, and type match.
/// - Stable stale-handle error otherwise.
///
/// Transformation:
/// - Rejects stale or forged handles before any native operation executes.
fn validate_handle(
    resources: &HashMap<u64, Resource>,
    handle: &NativeHandle,
) -> Result<(), NativeBridgeError> {
    match resources.get(&handle.id) {
        Some(resource)
            if resource.generation == handle.generation
                && resource.type_name == handle.type_name =>
        {
            Ok(())
        }
        _ => Err(NativeBridgeError::new(
            "stale_native_handle",
            format!(
                "native handle {} generation {} is not live",
                handle.id, handle.generation
            ),
        )),
    }
}

/// Executes a small observer operation for the probe.
///
/// Inputs:
/// - `operation`: requested method name.
///
/// Outputs:
/// - Typed value for known operations or a stable unsupported-operation error.
///
/// Transformation:
/// - Keeps the P0.4a worker independent from real Polars while proving typed
///   native calls can route through the actor bridge.
fn call_operation(operation: &str) -> Result<NativeValue, NativeBridgeError> {
    match operation {
        "height" | "width" => Ok(NativeValue::Int(0)),
        other => Err(NativeBridgeError::new(
            "unsupported_native_operation",
            format!("native operation `{other}` is not implemented by this probe"),
        )),
    }
}

mod tests {
    use super::*;

    /// Extracts a handle from an allocation reply.
    ///
    /// Inputs:
    /// - `reply`: worker allocation reply.
    ///
    /// Outputs:
    /// - Native handle stored in the reply.
    ///
    /// Transformation:
    /// - Panics only in tests if the worker contract returns the wrong value.
    fn handle_from(reply: NativeReply) -> NativeHandle {
        match reply.result.expect("allocation should succeed") {
            NativeValue::Handle(handle) => handle,
            other => panic!("expected handle, got {other:?}"),
        }
    }

    /// Verifies start/call/stop lifecycle with request ids and credits.
    ///
    /// Inputs:
    /// - Worker probe with a small credit window.
    ///
    /// Outputs:
    /// - Assertions over handle creation, observer call, request correlation,
    ///   and advertised credits.
    ///
    /// Transformation:
    /// - Exercises the VM-supervised actor bridge shape without native
    ///   package linkage.
    #[test]
    fn worker_allocates_handle_and_routes_typed_call() {
        let worker = SupervisedNativeWorker::start(4);
        let handle = handle_from(worker.allocate_dataframe(10));
        let reply = worker.call(11, handle, "height");

        assert_eq!(reply.request_id, 11);
        assert_eq!(reply.credits, 4);
        assert_eq!(reply.result, Ok(NativeValue::Int(0)));

        worker.stop();
    }

    /// Verifies disposed handles cannot be reused.
    ///
    /// Inputs:
    /// - Worker probe and one allocated handle.
    ///
    /// Outputs:
    /// - Assertions over successful disposal and stale-handle rejection.
    ///
    /// Transformation:
    /// - Proves generation-token based stale handle detection at the bridge
    ///   boundary.
    #[test]
    fn disposed_handle_is_rejected_as_stale() {
        let worker = SupervisedNativeWorker::start(DEFAULT_CREDIT_WINDOW);
        let handle = handle_from(worker.allocate_dataframe(20));

        assert_eq!(
            worker.dispose(21, handle.clone()).result,
            Ok(NativeValue::Unit)
        );

        let reply = worker.call(22, handle, "height");
        let err = reply.result.expect_err("disposed handle should fail");

        assert_eq!(err.code, "stale_native_handle");

        worker.stop();
    }

    /// Verifies unknown native operations fail with a stable error code.
    ///
    /// Inputs:
    /// - Worker probe and one allocated handle.
    ///
    /// Outputs:
    /// - Assertion over unsupported operation diagnostic.
    ///
    /// Transformation:
    /// - Ensures bridge errors are explicit before real adapter operations are
    ///   implemented.
    #[test]
    fn unsupported_operation_returns_stable_error() {
        let worker = SupervisedNativeWorker::start(DEFAULT_CREDIT_WINDOW);
        let handle = handle_from(worker.allocate_dataframe(30));
        let reply = worker.call(31, handle, "select");
        let err = reply
            .result
            .expect_err("select is not implemented by probe");

        assert_eq!(err.code, "unsupported_native_operation");

        worker.stop();
    }
}
"#;

pub(in crate::commands::bind) const POLARS_FILES: &[GeneratedFile] = &[
    GeneratedFile {
        path: "terlan.toml",
        contents: POLARS_TOML,
    },
    GeneratedFile {
        path: "src/std/native/polars/DataFrame.terl",
        contents: POLARS_DATAFRAME_TN,
    },
    GeneratedFile {
        path: "bindings/polars.mapping.toml",
        contents: POLARS_MAPPING_TOML,
    },
    GeneratedFile {
        path: "native/terlan-native.toml",
        contents: POLARS_NATIVE_ABI_TOML,
    },
    GeneratedFile {
        path: "docs/std.native.polars.md",
        contents: POLARS_PACKAGE_DOC,
    },
    GeneratedFile {
        path: "examples/read_csv.terl",
        contents: POLARS_READ_CSV_EXAMPLE,
    },
    GeneratedFile {
        path: "summaries/std.native.polars.DataFrame.typi",
        contents: POLARS_DATAFRAME_TYPI,
    },
    GeneratedFile {
        path: "native/rust/Cargo.toml",
        contents: POLARS_RUST_CARGO_TOML,
    },
    GeneratedFile {
        path: "native/rust/src/lib.rs",
        contents: POLARS_RUST_STUB,
    },
    GeneratedFile {
        path: "native/rust/src/bridge.rs",
        contents: POLARS_RUST_BRIDGE,
    },
];
