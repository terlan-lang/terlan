//! Durable source-child snapshot and event hooks reuse the VM store contract.

use super::*;
use crate::runtime::vm::persistent_actor_store::*;

const MAX_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
const MAX_STORE_BYTES: u64 = 64 * 1024 * 1024;

pub(super) fn validate(recovery: &ReplValue) -> Result<(), VmSupervisionError> {
    if text(recovery, "path", 1)?.is_empty() {
        return Err(VmSupervisionError::new(
            Configuration,
            "recovery path must be nonempty",
        ));
    }
    VmPersistentActorSchema::new(
        text(recovery, "schema", 2)?,
        nonnegative(recovery, "version", 3)?,
    )
    .map_err(|message| VmSupervisionError::new(Configuration, message))?;
    Ok(())
}

impl VmSourceSupervision {
    /// Opens the logical child's durable store independently of process identity.
    pub(crate) fn persistence(
        &self,
        owner: VmProcessId,
        operation: &str,
        arguments: &[ReplValue],
    ) -> Result<ReplValue, VmSupervisionError> {
        let (key, child) = self
            .children
            .iter()
            .find(|(key, _)| self.system.supervisors[&key.0].children[&key.1].pid == owner)
            .ok_or_else(|| {
                VmSupervisionError::new(
                    Configuration,
                    "persistence operation requires a supervised child",
                )
            })?;
        let recovery = child.recovery.as_ref().ok_or_else(|| {
            VmSupervisionError::new(Configuration, "child has no durable recovery binding")
        })?;
        let path = text(recovery, "path", 1)?;
        match std::fs::metadata(path) {
            Ok(metadata) if metadata.len() > MAX_STORE_BYTES => {
                return Err(VmSupervisionError::new(
                    Configuration,
                    "supervised child store exceeds 64 MiB",
                ))
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(VmSupervisionError::new(
                    Configuration,
                    format!("cannot inspect recovery store: {error}"),
                ))
            }
        }
        let mut store = VmFileBackedPersistentActorStore::open_file_backed(path)
            .map_err(|message| VmSupervisionError::new(Configuration, message))?;
        let actor = VmPersistentActorId::new(format!(
            "supervisor:{}/child:{}",
            int(&self.roots[&key.0].policy, "name", 1)?,
            key.1
        ))
        .map_err(|message| VmSupervisionError::new(Configuration, message))?;
        let schema = VmPersistentActorSchema::new(
            text(recovery, "schema", 2)?,
            nonnegative(recovery, "version", 3)?,
        )
        .map_err(|message| VmSupervisionError::new(Configuration, message))?;
        match (operation, arguments) {
            ("checkpoint", [ReplValue::Int(sequence), state @ ReplValue::Bytes(bytes)])
                if *sequence >= 0 =>
            {
                if bytes.len() > MAX_PAYLOAD_BYTES {
                    return Err(VmSupervisionError::new(
                        Configuration,
                        "snapshot exceeds 16 MiB",
                    ));
                }
                let generation = store
                    .load_snapshot(&actor)
                    .map_or(Some(1), |snapshot| snapshot.generation.checked_add(1))
                    .ok_or_else(|| {
                        VmSupervisionError::new(Configuration, "snapshot generation exhausted")
                    })?;
                let snapshot = VmPersistentActorSnapshot::new(
                    actor,
                    schema,
                    generation,
                    state.clone(),
                    Vec::new(),
                    Vec::new(),
                    VmPersistentActorDurability {
                        resource_handles: Vec::new(),
                        last_event_sequence: *sequence as u64,
                    },
                )
                .map_err(|message| VmSupervisionError::new(Configuration, message))?;
                match store.store_snapshot(snapshot) {
                    VmPersistentActorStoreOutcome::SnapshotStored(_) => Ok(ReplValue::Unit),
                    outcome => Err(VmSupervisionError::new(
                        Configuration,
                        format!("snapshot rejected: {outcome:?}"),
                    )),
                }
            }
            ("append_event", [ReplValue::Int(sequence), payload @ ReplValue::Bytes(bytes)])
                if *sequence > 0 =>
            {
                if bytes.len() > MAX_PAYLOAD_BYTES {
                    return Err(VmSupervisionError::new(
                        Configuration,
                        "event exceeds 16 MiB",
                    ));
                }
                if store.load_snapshot(&actor).is_none() {
                    return Err(VmSupervisionError::new(
                        Configuration,
                        "checkpoint must precede event append",
                    ));
                }
                let event =
                    VmPersistentActorEvent::new(actor, schema, *sequence as u64, payload.clone())
                        .map_err(|message| VmSupervisionError::new(Configuration, message))?;
                match store.append_event(event) {
                    VmPersistentActorStoreOutcome::EventAppended(_)
                    | VmPersistentActorStoreOutcome::Replayed(_) => Ok(ReplValue::Unit),
                    outcome => Err(VmSupervisionError::new(
                        Configuration,
                        format!("event rejected: {outcome:?}"),
                    )),
                }
            }
            ("restore", []) => {
                let replay = store.replay(&actor, &schema).map_err(|outcome| {
                    VmSupervisionError::new(Configuration, format!("restore rejected: {outcome:?}"))
                })?;
                Ok(record(
                    "RecoveryState",
                    vec![
                        (
                            "generation",
                            ReplValue::Int(replay.snapshot.generation as i64),
                        ),
                        (
                            "sequence",
                            ReplValue::Int(replay.snapshot.last_event_sequence as i64),
                        ),
                        ("state", replay.snapshot.state),
                        (
                            "events",
                            ReplValue::List(
                                replay
                                    .events
                                    .into_iter()
                                    .map(|event| {
                                        record(
                                            "PersistedEvent",
                                            vec![
                                                ("sequence", ReplValue::Int(event.sequence as i64)),
                                                ("payload", event.payload),
                                            ],
                                        )
                                    })
                                    .collect(),
                            ),
                        ),
                    ],
                ))
            }
            _ => Err(VmSupervisionError::new(
                Configuration,
                "invalid supervision persistence arguments",
            )),
        }
    }
}

fn text<'a>(value: &'a ReplValue, name: &str, index: usize) -> Result<&'a str, VmSupervisionError> {
    match field(value, name, index)? {
        ReplValue::String(value) => Ok(value),
        _ => Err(VmSupervisionError::new(
            Configuration,
            format!("`{name}` must be String"),
        )),
    }
}
