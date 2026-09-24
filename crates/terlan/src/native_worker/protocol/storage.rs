//! Durable operations executed only by a dedicated blocking capability worker.
//!
//! Requests carry checkpoint data, never paths, SQL, actor heaps, or handles.
//! Cancellation or transport loss after mutation is an indeterminate commit;
//! callers must inspect/replay the identical checkpoint rather than assume rollback.

use std::path::PathBuf;

use terlan_storage::{AppendOutcome, Checkpoint, CheckpointStore, StorageError};

use crate::terlan_native_boundary::cancellation::NativeBoundaryCancellationToken;
use crate::terlan_native_boundary::capability_wire::{
    CapabilityOutcome, CapabilityResponse, CapabilityValue, CAPABILITY_PROTOCOL_VERSION,
};
use crate::terlan_native_boundary::request::RequestId;
use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm;
use crate::terlan_native_boundary::worker::{NativeBoundaryWorker, NativeBoundaryWorkerReply};

use super::execution::CapabilityCall;
use super::CapabilityWorkerConfig;

/// Only database location admitted inside the worker's explicit durable mount.
pub(super) const DATABASE_PATH: &str = "/storage/checkpoints.sqlite";

/// Returns the closed internal storage operation set, not a source API claim.
pub(super) fn admits(operation: &str) -> bool {
    matches!(
        operation,
        "runtime.storage.sequence"
            | "runtime.storage.open"
            | "runtime.storage.append"
            | "runtime.storage.load"
            | "runtime.storage.compact"
            | "runtime.storage.status"
            | "runtime.storage.flush"
            | "runtime.storage.migrate_schema"
    )
}

/// One connection owned exclusively by the worker executor thread.
pub(super) struct StorageExecutor {
    path: Option<PathBuf>,
    database: Option<CheckpointStore>,
    max_frame_bytes: usize,
}

impl StorageExecutor {
    /// Retains startup authority without opening the database on the coordinator.
    pub(super) fn new(config: &CapabilityWorkerConfig) -> Self {
        Self {
            path: config
                .storage_database
                .then(|| PathBuf::from(DATABASE_PATH)),
            database: None,
            max_frame_bytes: config.max_payload_bytes,
        }
    }

    /// Reuses worker identity/credit accounting without storing SQLite in cloneable resources.
    pub(super) fn call(
        &mut self,
        worker: &mut NativeBoundaryWorker,
        call: CapabilityCall,
        cancellation: &NativeBoundaryCancellationToken,
    ) -> NativeBoundaryWorkerReply {
        let id = RequestId {
            value: call.request_id,
        };
        let result = match worker.begin_request(id) {
            Err(error) => error,
            Ok(()) => {
                let reply = if cancellation.is_cancelled() {
                    Err(StorageError::Invalid("cancelled before execution"))
                } else {
                    self.execute(&call.operation, call.arguments)
                };
                if cancellation.is_cancelled() {
                    worker.cancel_request(id)
                } else {
                    match worker.finish_request(id) {
                        Err(error) => error,
                        Ok(()) => reply.map_or_else(error_reply, |value| {
                            NativeBoundaryReplyTerm::Ok(value.into_term())
                        }),
                    }
                }
            }
        };
        NativeBoundaryWorkerReply {
            request_id: id,
            result,
            reserved_credits: worker.reserved_credits(),
            available_credits: worker.available_credits(),
        }
    }

    /// Decodes the entire operation before opening/mutating the authorized database.
    fn execute(
        &mut self,
        operation: &str,
        arguments: Vec<CapabilityValue>,
    ) -> Result<CapabilityValue, StorageError> {
        let request = Request::decode(operation, arguments)?;
        let path = self.path.as_ref().ok_or(StorageError::Invalid(
            "no supervisor-authorized durable binding",
        ))?;
        if let Request::Append(_, checkpoints) = &request {
            for checkpoint in checkpoints {
                require_reply_fits(checkpoint_value(checkpoint.clone()), self.max_frame_bytes)?;
            }
        }
        if self.database.is_none() {
            self.database = Some(CheckpointStore::open(path)?);
        }
        let database = self
            .database
            .as_mut()
            .ok_or(StorageError::Invalid("database unavailable"))?;
        let value = match request {
            Request::Open => {
                let observation = database.observation()?;
                CapabilityValue::Tuple(vec![
                    CapabilityValue::Bytes(observation.identity.to_vec()),
                    status_value(observation.status)?,
                ])
            }
            Request::Sequence => CapabilityValue::Int(integer(database.sequence()?)?),
            Request::Status => status_value(database.status()?)?,
            Request::Flush => status_value(database.flush()?)?,
            Request::MigrateSchema(expected, next) => {
                status_value(database.migrate_schema(expected, next)?)?
            }
            Request::Append(expected, checkpoints) => CapabilityValue::Bool(
                database.append(expected, &checkpoints)? == AppendOutcome::Committed,
            ),
            Request::Load(id) => match database.load(&id)? {
                Some(checkpoint) => checkpoint_value(checkpoint),
                None => CapabilityValue::List(Vec::new()),
            },
            Request::Compact(boundary) => {
                let outcome = database.compact(boundary)?;
                CapabilityValue::Tuple(vec![
                    CapabilityValue::Int(
                        i64::try_from(outcome.removed).map_err(|_| StorageError::Corrupt)?,
                    ),
                    CapabilityValue::Int(integer(outcome.retained)?),
                    CapabilityValue::Int(integer(outcome.sequence)?),
                ])
            }
        };
        require_reply_fits(value.clone(), self.max_frame_bytes)?;
        Ok(value)
    }
}

/// Fully decoded request; malformed batches cannot partially execute.
enum Request {
    Open,
    Sequence,
    Status,
    Flush,
    MigrateSchema(u32, u32),
    Append(u64, Vec<Checkpoint>),
    Load(String),
    Compact(u64),
}

impl Request {
    /// Accepts a closed typed shape with no SQL or filesystem interpretation.
    fn decode(operation: &str, args: Vec<CapabilityValue>) -> Result<Self, StorageError> {
        let mut args = args.into_iter();
        let request = match (operation, args.next(), args.next()) {
            ("runtime.storage.open", None, None) => Self::Open,
            ("runtime.storage.sequence", None, None) => Self::Sequence,
            ("runtime.storage.status", None, None) => Self::Status,
            ("runtime.storage.flush", None, None) => Self::Flush,
            (
                "runtime.storage.migrate_schema",
                Some(CapabilityValue::Int(expected)),
                Some(CapabilityValue::Int(next)),
            ) => {
                if expected <= 0 || next <= expected {
                    return Err(StorageError::Invalid(
                        "schema versions must be nonzero and increase",
                    ));
                }
                Self::MigrateSchema(
                    u32::try_from(expected)
                        .map_err(|_| StorageError::Invalid("expected schema range"))?,
                    u32::try_from(next).map_err(|_| StorageError::Invalid("next schema range"))?,
                )
            }
            ("runtime.storage.load", Some(CapabilityValue::Text(id)), None) => Self::Load(id),
            ("runtime.storage.compact", Some(CapabilityValue::Int(boundary)), None) => {
                Self::Compact(non_negative(boundary)?)
            }
            (
                "runtime.storage.append",
                Some(CapabilityValue::Int(expected)),
                Some(CapabilityValue::List(values)),
            ) => {
                if values.is_empty() || values.len() > terlan_storage::MAX_BATCH_CHECKPOINTS {
                    return Err(StorageError::Invalid("checkpoint batch count"));
                }
                let checkpoints = values
                    .into_iter()
                    .map(decode_checkpoint)
                    .collect::<Result<Vec<_>, _>>()?;
                Self::Append(non_negative(expected)?, checkpoints)
            }
            _ => return Err(StorageError::Invalid("operation or argument shape")),
        };
        if args.next().is_some() {
            return Err(StorageError::Invalid("operation arity"));
        }
        Ok(request)
    }
}

/// Moves bounded bytes out of the request instead of copying the batch.
fn decode_checkpoint(value: CapabilityValue) -> Result<Checkpoint, StorageError> {
    let CapabilityValue::Tuple(fields) = value else {
        return Err(StorageError::Invalid("checkpoint must be a tuple"));
    };
    let mut fields = fields.into_iter();
    let (
        Some(CapabilityValue::Text(id)),
        Some(CapabilityValue::Int(sequence)),
        Some(CapabilityValue::Int(schema)),
        Some(CapabilityValue::Bytes(payload)),
        None,
    ) = (
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
    )
    else {
        return Err(StorageError::Invalid("checkpoint tuple shape"));
    };
    let checkpoint = Checkpoint {
        id,
        sequence: non_negative(sequence)?,
        schema: u32::try_from(schema).map_err(|_| StorageError::Invalid("schema range"))?,
        payload,
    };
    checkpoint.validate()?;
    Ok(checkpoint)
}

/// Encodes a present logical checkpoint; absence is an empty list.
fn checkpoint_value(checkpoint: Checkpoint) -> CapabilityValue {
    CapabilityValue::List(vec![CapabilityValue::Tuple(vec![
        CapabilityValue::Text(checkpoint.id),
        CapabilityValue::Int(checkpoint.sequence as i64),
        CapabilityValue::Int(i64::from(checkpoint.schema)),
        CapabilityValue::Bytes(checkpoint.payload),
    ])])
}

/// Reserves the largest possible correlation/credit envelope within the frame limit.
fn require_reply_fits(value: CapabilityValue, maximum: usize) -> Result<(), StorageError> {
    let response = CapabilityResponse::Reply {
        version: CAPABILITY_PROTOCOL_VERSION,
        request_id: u64::MAX,
        reserved_credits: u64::MAX,
        available_credits: u64::MAX,
        outcome: CapabilityOutcome::Ok { value },
    };
    crate::terlan_native_boundary::capability_wire::write_json_frame(
        &mut std::io::sink(),
        &response,
        maximum,
    )
    .map_err(|_| StorageError::Invalid("checkpoint reply exceeds worker frame limit"))
}

fn non_negative(value: i64) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| StorageError::Invalid("negative sequence"))
}

fn integer(value: u64) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| StorageError::Corrupt)
}

/// Carries one consistent durable schema/sequence observation across the existing wire.
fn status_value(status: terlan_storage::StorageStatus) -> Result<CapabilityValue, StorageError> {
    Ok(CapabilityValue::Tuple(vec![
        CapabilityValue::Int(integer(status.sequence)?),
        CapabilityValue::Int(i64::from(status.schema)),
        CapabilityValue::Int(integer(status.schema_sequence)?),
    ]))
}

/// Keeps native database diagnostics out of the VM reply while preserving error categories.
fn error_reply(error: StorageError) -> NativeBoundaryReplyTerm {
    use crate::terlan_native_boundary::storage_reply::StorageFailure;
    let failure = StorageFailure::from(error);
    failure.into_term().map_or_else(
        |_| NativeBoundaryReplyTerm::Error {
            code: "storage.reply".into(),
            message: "invalid storage failure metadata; reconcile durable state".into(),
            offset: 0,
        },
        NativeBoundaryReplyTerm::Ok,
    )
}

#[cfg(test)]
#[path = "storage/storage_test.rs"]
mod tests;
