#![forbid(unsafe_code)]
// AUTO-GENERATED NativeBoundary skeleton.
// Implement concrete native exports only after preserving this bridge contract.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

pub const SOURCE_MODULE: &str = "std.vm.DistributedStorage";
pub const NATIVE_MODULE: &str = "std_vm_distributed_storage_native_boundary";
pub const SCHEDULER: &str = "normal";

pub const FUNCTIONS: &[(&str, usize)] = &[
    ("local_only", 0),
    ("durable", 0),
    ("cluster", 0),
    ("force_local", 0),
    ("policy", 3),
    ("policy_name", 1),
    ("policy_mode_kind", 1),
    ("policy_available", 1),
    ("policy_can_cluster_replicate", 1),
    ("adapter", 1),
    ("policy_name", 1),
    ("policy_mode_kind", 1),
    ("policy_available", 1),
    ("can_cluster_replicate", 1),
    ("checkpoint", 3),
    ("restore", 1),
    ("checkpoint_with_schema", 4),
    ("checkpoint_schema", 1),
    ("open", 1),
    ("append", 2),
    ("require_atomic_append", 1),
    ("atomic_append_proof", 1),
    ("require_snapshot_isolation", 1),
    ("snapshot_isolation_proof", 2),
    ("require_durable_flush", 1),
    ("durable_flush_proof", 1),
    ("require_transactional_batch", 1),
    ("transactional_batch_proof", 1),
    ("transactional_batch_append", 2),
    ("require_schema_migration", 1),
    ("schema_migration_proof", 1),
    ("migrate_schema", 3),
    ("require_resource_handle_validation", 1),
    ("resource_handle_validation_proof", 1),
    ("register_resource_handle", 2),
    ("validate_resource_handles", 2),
    ("replicate_snapshot", 2),
    ("compare_and_swap_token", 1),
    ("compare_and_swap_append", 3),
    ("flush", 1),
    ("compact", 2),
    ("load_snapshot", 2),
    ("close", 1),
    ("require_cluster_replication", 1),
    ("proof_sequence", 1),
    ("isolation_checkpoint_id", 1),
    ("isolation_sequence", 1),
    ("isolation_checksum", 1),
    ("durable_flush_sequence", 1),
    ("batch_first_sequence", 1),
    ("batch_last_sequence", 1),
    ("batch_committed_count", 1),
    ("schema_version", 1),
    ("schema_sequence", 1),
    ("resource_handle_count", 1),
    ("resource_handle_sequence", 1),
    ("missing_resource_handle", 1),
    ("validated_resource_count", 1),
    ("loaded_snapshot", 1),
    ("kind", 1),
    ("operation_kind", 1),
    ("mode_kind", 1),
    ("reason", 1),
    ("is_failure", 1),
    ("is_success", 1),
    ("requires_recovery", 1),
    ("recovery_action", 1),
    ("sequence", 1),
    ("expected_sequence", 1),
    ("actual_sequence", 1),
    ("expected_schema", 1),
    ("actual_schema", 1),
    ("checkpoint_id", 1),
    ("local_sequence", 1),
    ("incoming_sequence", 1),
    ("checksum", 1),
    ("expected_checksum", 1),
    ("expected_entries", 1),
    ("persisted_entries", 1),
    ("retained_snapshots", 1),
];

pub const OPERATIONS: &[(&str, &str, usize)] = &[
    ("local_only", "std.vm.distributed_storage.local_only", 0),
    ("durable", "std.vm.distributed_storage.durable", 0),
    ("cluster", "std.vm.distributed_storage.cluster", 0),
    ("force_local", "std.vm.distributed_storage.force_local", 0),
    ("policy", "std.vm.distributed_storage.policy", 3),
    ("policy_name", "std.vm.distributed_storage.policy_name", 1),
    ("policy_mode_kind", "std.vm.distributed_storage.policy_mode_kind", 1),
    ("policy_available", "std.vm.distributed_storage.policy_available", 1),
    ("policy_can_cluster_replicate", "std.vm.distributed_storage.policy_can_cluster_replicate", 1),
    ("adapter", "std.vm.distributed_storage.adapter", 1),
    ("policy_name", "std.vm.distributed_storage.adapter_policy_name", 1),
    ("policy_mode_kind", "std.vm.distributed_storage.adapter_policy_mode_kind", 1),
    ("policy_available", "std.vm.distributed_storage.adapter_policy_available", 1),
    ("can_cluster_replicate", "std.vm.distributed_storage.can_cluster_replicate", 1),
    ("checkpoint", "std.vm.distributed_storage.checkpoint", 3),
    ("restore", "std.vm.distributed_storage.restore", 1),
    ("checkpoint_with_schema", "std.vm.distributed_storage.checkpoint_with_schema", 4),
    ("checkpoint_schema", "std.vm.distributed_storage.checkpoint_schema", 1),
    ("open", "std.vm.distributed_storage.open", 1),
    ("append", "std.vm.distributed_storage.append", 2),
    ("require_atomic_append", "std.vm.distributed_storage.require_atomic_append", 1),
    ("atomic_append_proof", "std.vm.distributed_storage.atomic_append_proof", 1),
    ("require_snapshot_isolation", "std.vm.distributed_storage.require_snapshot_isolation", 1),
    ("snapshot_isolation_proof", "std.vm.distributed_storage.snapshot_isolation_proof", 2),
    ("require_durable_flush", "std.vm.distributed_storage.require_durable_flush", 1),
    ("durable_flush_proof", "std.vm.distributed_storage.durable_flush_proof", 1),
    ("require_transactional_batch", "std.vm.distributed_storage.require_transactional_batch", 1),
    ("transactional_batch_proof", "std.vm.distributed_storage.transactional_batch_proof", 1),
    ("transactional_batch_append", "std.vm.distributed_storage.transactional_batch_append", 2),
    ("require_schema_migration", "std.vm.distributed_storage.require_schema_migration", 1),
    ("schema_migration_proof", "std.vm.distributed_storage.schema_migration_proof", 1),
    ("migrate_schema", "std.vm.distributed_storage.migrate_schema", 3),
    ("require_resource_handle_validation", "std.vm.distributed_storage.require_resource_handle_validation", 1),
    ("resource_handle_validation_proof", "std.vm.distributed_storage.resource_handle_validation_proof", 1),
    ("register_resource_handle", "std.vm.distributed_storage.register_resource_handle", 2),
    ("validate_resource_handles", "std.vm.distributed_storage.validate_resource_handles", 2),
    ("replicate_snapshot", "std.vm.distributed_storage.replicate_snapshot", 2),
    ("compare_and_swap_token", "std.vm.distributed_storage.compare_and_swap_token", 1),
    ("compare_and_swap_append", "std.vm.distributed_storage.compare_and_swap_append", 3),
    ("flush", "std.vm.distributed_storage.flush", 1),
    ("compact", "std.vm.distributed_storage.compact", 2),
    ("load_snapshot", "std.vm.distributed_storage.load_snapshot", 2),
    ("close", "std.vm.distributed_storage.close", 1),
    ("require_cluster_replication", "std.vm.distributed_storage.require_cluster_replication", 1),
    ("proof_sequence", "std.vm.distributed_storage.proof_sequence", 1),
    ("isolation_checkpoint_id", "std.vm.distributed_storage.isolation_checkpoint_id", 1),
    ("isolation_sequence", "std.vm.distributed_storage.isolation_sequence", 1),
    ("isolation_checksum", "std.vm.distributed_storage.isolation_checksum", 1),
    ("durable_flush_sequence", "std.vm.distributed_storage.durable_flush_sequence", 1),
    ("batch_first_sequence", "std.vm.distributed_storage.batch_first_sequence", 1),
    ("batch_last_sequence", "std.vm.distributed_storage.batch_last_sequence", 1),
    ("batch_committed_count", "std.vm.distributed_storage.batch_committed_count", 1),
    ("schema_version", "std.vm.distributed_storage.schema_version", 1),
    ("schema_sequence", "std.vm.distributed_storage.schema_sequence", 1),
    ("resource_handle_count", "std.vm.distributed_storage.resource_handle_count", 1),
    ("resource_handle_sequence", "std.vm.distributed_storage.resource_handle_sequence", 1),
    ("missing_resource_handle", "std.vm.distributed_storage.missing_resource_handle", 1),
    ("validated_resource_count", "std.vm.distributed_storage.validated_resource_count", 1),
    ("loaded_snapshot", "std.vm.distributed_storage.loaded_snapshot", 1),
    ("kind", "std.vm.distributed_storage.kind", 1),
    ("operation_kind", "std.vm.distributed_storage.operation_kind", 1),
    ("mode_kind", "std.vm.distributed_storage.mode_kind", 1),
    ("reason", "std.vm.distributed_storage.reason", 1),
    ("is_failure", "std.vm.distributed_storage.is_failure", 1),
    ("is_success", "std.vm.distributed_storage.is_success", 1),
    ("requires_recovery", "std.vm.distributed_storage.requires_recovery", 1),
    ("recovery_action", "std.vm.distributed_storage.recovery_action", 1),
    ("sequence", "std.vm.distributed_storage.sequence", 1),
    ("expected_sequence", "std.vm.distributed_storage.expected_sequence", 1),
    ("actual_sequence", "std.vm.distributed_storage.actual_sequence", 1),
    ("expected_schema", "std.vm.distributed_storage.expected_schema", 1),
    ("actual_schema", "std.vm.distributed_storage.actual_schema", 1),
    ("checkpoint_id", "std.vm.distributed_storage.checkpoint_id", 1),
    ("local_sequence", "std.vm.distributed_storage.local_sequence", 1),
    ("incoming_sequence", "std.vm.distributed_storage.incoming_sequence", 1),
    ("checksum", "std.vm.distributed_storage.checksum", 1),
    ("expected_checksum", "std.vm.distributed_storage.expected_checksum", 1),
    ("expected_entries", "std.vm.distributed_storage.expected_entries", 1),
    ("persisted_entries", "std.vm.distributed_storage.persisted_entries", 1),
    ("retained_snapshots", "std.vm.distributed_storage.retained_snapshots", 1),
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
                        "std.vm.distributed_storage.local_only" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.durable" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.cluster" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.force_local" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.policy" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.policy_name" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.policy_mode_kind" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.policy_available" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.policy_can_cluster_replicate" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.adapter" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.adapter_policy_name" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.adapter_policy_mode_kind" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.adapter_policy_available" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.can_cluster_replicate" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.checkpoint" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.restore" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.checkpoint_with_schema" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.checkpoint_schema" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.open" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.append" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.require_atomic_append" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.atomic_append_proof" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.require_snapshot_isolation" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.snapshot_isolation_proof" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.require_durable_flush" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.durable_flush_proof" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.require_transactional_batch" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.transactional_batch_proof" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.transactional_batch_append" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.require_schema_migration" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.schema_migration_proof" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.migrate_schema" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.require_resource_handle_validation" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.resource_handle_validation_proof" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.register_resource_handle" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.validate_resource_handles" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.replicate_snapshot" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.compare_and_swap_token" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.compare_and_swap_append" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.flush" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.compact" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.load_snapshot" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.close" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.require_cluster_replication" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.proof_sequence" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.isolation_checkpoint_id" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.isolation_sequence" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.isolation_checksum" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.durable_flush_sequence" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.batch_first_sequence" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.batch_last_sequence" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.batch_committed_count" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.schema_version" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.schema_sequence" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.resource_handle_count" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.resource_handle_sequence" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.missing_resource_handle" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.validated_resource_count" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.loaded_snapshot" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.kind" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.operation_kind" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.mode_kind" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.reason" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.is_failure" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.is_success" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.requires_recovery" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.recovery_action" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.sequence" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.expected_sequence" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.actual_sequence" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.expected_schema" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.actual_schema" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.checkpoint_id" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.local_sequence" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.incoming_sequence" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.checksum" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.expected_checksum" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.expected_entries" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.persisted_entries" => native_unimplemented_operation(operation),
                        "std.vm.distributed_storage.retained_snapshots" => native_unimplemented_operation(operation),
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
