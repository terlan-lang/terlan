use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::ReplValue;

/// Stable VM-owned identity for one persistent actor instance.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmPersistentActorId(String);

impl VmPersistentActorId {
    pub(crate) fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.is_empty() {
            return Err("error[vm_persistent_actor]: actor id must be non-empty".to_string());
        }
        Ok(Self(value))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Schema identity that must match before replay can restore actor state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmPersistentActorSchema {
    pub(crate) id: String,
    pub(crate) version: u64,
}

impl VmPersistentActorSchema {
    pub(crate) fn new(id: impl Into<String>, version: u64) -> Result<Self, String> {
        let id = id.into();
        if id.is_empty() {
            return Err("error[vm_persistent_actor]: schema id must be non-empty".to_string());
        }
        if version == 0 {
            return Err("error[vm_persistent_actor]: schema version must be non-zero".to_string());
        }
        Ok(Self { id, version })
    }
}

/// Source-visible declaration that binds one actor family to its durable schema.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmPersistentActorDeclaration {
    pub(crate) actor_id: VmPersistentActorId,
    pub(crate) schema: VmPersistentActorSchema,
    pub(crate) storage_lane: String,
}

impl VmPersistentActorDeclaration {
    pub(crate) fn new(
        actor_id: VmPersistentActorId,
        schema: VmPersistentActorSchema,
        storage_lane: impl Into<String>,
    ) -> Result<Self, String> {
        let storage_lane = storage_lane.into();
        if storage_lane.is_empty() {
            return Err(
                "error[vm_persistent_actor]: persistent actor storage lane must be non-empty"
                    .to_string(),
            );
        }
        if !storage_lane.contains(&actor_id.0) || !storage_lane.contains(&schema.id) {
            return Err(format!(
                "error[vm_persistent_actor]: persistent actor storage lane `{storage_lane}` must include actor `{}` and schema `{}`",
                actor_id.0, schema.id
            ));
        }
        Ok(Self {
            actor_id,
            schema,
            storage_lane,
        })
    }
}

/// Durable actor snapshot checkpoint.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmPersistentActorSnapshot {
    pub(crate) actor_id: VmPersistentActorId,
    pub(crate) schema: VmPersistentActorSchema,
    pub(crate) generation: u64,
    pub(crate) state: ReplValue,
    pub(crate) mailbox_checkpoint: Vec<ReplValue>,
    pub(crate) timer_checkpoint: Vec<u64>,
    pub(crate) resource_handles: Vec<String>,
    pub(crate) last_event_sequence: u64,
}

impl VmPersistentActorSnapshot {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        actor_id: VmPersistentActorId,
        schema: VmPersistentActorSchema,
        generation: u64,
        state: ReplValue,
        mailbox_checkpoint: Vec<ReplValue>,
        timer_checkpoint: Vec<u64>,
        resource_handles: Vec<String>,
        last_event_sequence: u64,
    ) -> Result<Self, String> {
        if generation == 0 {
            return Err(
                "error[vm_persistent_actor]: snapshot generation must be non-zero".to_string(),
            );
        }
        if resource_handles.iter().any(|handle| handle.is_empty()) {
            return Err(
                "error[vm_persistent_actor]: resource handles must be non-empty".to_string(),
            );
        }
        Ok(Self {
            actor_id,
            schema,
            generation,
            state,
            mailbox_checkpoint,
            timer_checkpoint,
            resource_handles,
            last_event_sequence,
        })
    }
}

/// Append-only actor event committed after the latest durable snapshot.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmPersistentActorEvent {
    pub(crate) actor_id: VmPersistentActorId,
    pub(crate) schema: VmPersistentActorSchema,
    pub(crate) sequence: u64,
    pub(crate) payload: ReplValue,
}

impl VmPersistentActorEvent {
    pub(crate) fn new(
        actor_id: VmPersistentActorId,
        schema: VmPersistentActorSchema,
        sequence: u64,
        payload: ReplValue,
    ) -> Result<Self, String> {
        if sequence == 0 {
            return Err("error[vm_persistent_actor]: event sequence must be non-zero".to_string());
        }
        Ok(Self {
            actor_id,
            schema,
            sequence,
            payload,
        })
    }
}

/// Result of replaying the durable actor checkpoint and committed events.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmPersistentActorReplay {
    pub(crate) snapshot: VmPersistentActorSnapshot,
    pub(crate) events: Vec<VmPersistentActorEvent>,
}

/// Typed store result for writes and replay setup.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum VmPersistentActorStoreOutcome {
    SnapshotStored(VmPersistentActorSnapshot),
    EventAppended(VmPersistentActorEvent),
    Replayed(VmPersistentActorEvent),
    MissingSnapshot(VmPersistentActorId),
    StaleSnapshot {
        actor_id: VmPersistentActorId,
        current_generation: u64,
        incoming_generation: u64,
    },
    StaleEvent {
        actor_id: VmPersistentActorId,
        current_sequence: u64,
        incoming_sequence: u64,
    },
    DuplicateEvent {
        actor_id: VmPersistentActorId,
        sequence: u64,
    },
    IncompatibleSchema {
        actor_id: VmPersistentActorId,
        expected: VmPersistentActorSchema,
        actual: VmPersistentActorSchema,
    },
    PartialWriteRejected {
        actor_id: VmPersistentActorId,
        sequence: u64,
    },
    PersistenceFailed {
        actor_id: VmPersistentActorId,
        reason: String,
    },
}

/// Explicit adapter contract shared by VM-owned persistent actor stores.
pub(crate) trait VmPersistentActorStoreAdapter {
    fn store_snapshot(
        &mut self,
        snapshot: VmPersistentActorSnapshot,
    ) -> VmPersistentActorStoreOutcome;
    fn append_event(&mut self, event: VmPersistentActorEvent) -> VmPersistentActorStoreOutcome;
    fn reject_partial_event(
        &mut self,
        event: VmPersistentActorEvent,
    ) -> VmPersistentActorStoreOutcome;
    fn load_snapshot(&self, actor_id: &VmPersistentActorId) -> Option<&VmPersistentActorSnapshot>;
    fn events_after(
        &self,
        actor_id: &VmPersistentActorId,
        sequence: u64,
    ) -> Vec<VmPersistentActorEvent>;
    fn replay(
        &self,
        actor_id: &VmPersistentActorId,
        expected_schema: &VmPersistentActorSchema,
    ) -> Result<VmPersistentActorReplay, VmPersistentActorStoreOutcome>;
}

/// Deterministic in-memory adapter used by VM replay and adversarial tests.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct VmInMemoryPersistentActorStore {
    snapshots: BTreeMap<VmPersistentActorId, VmPersistentActorSnapshot>,
    events: BTreeMap<VmPersistentActorId, BTreeMap<u64, VmPersistentActorEvent>>,
}

impl VmInMemoryPersistentActorStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn latest_event_sequence(&self, actor_id: &VmPersistentActorId) -> u64 {
        self.events
            .get(actor_id)
            .and_then(|events| events.keys().next_back().copied())
            .unwrap_or(0)
    }

    fn incompatible_schema(
        actor_id: VmPersistentActorId,
        expected: VmPersistentActorSchema,
        actual: VmPersistentActorSchema,
    ) -> VmPersistentActorStoreOutcome {
        VmPersistentActorStoreOutcome::IncompatibleSchema {
            actor_id,
            expected,
            actual,
        }
    }
}

impl VmPersistentActorStoreAdapter for VmInMemoryPersistentActorStore {
    fn store_snapshot(
        &mut self,
        snapshot: VmPersistentActorSnapshot,
    ) -> VmPersistentActorStoreOutcome {
        if let Some(current) = self.snapshots.get(&snapshot.actor_id) {
            if current.schema != snapshot.schema {
                return Self::incompatible_schema(
                    snapshot.actor_id,
                    current.schema.clone(),
                    snapshot.schema,
                );
            }
            if snapshot.generation <= current.generation {
                return VmPersistentActorStoreOutcome::StaleSnapshot {
                    actor_id: snapshot.actor_id,
                    current_generation: current.generation,
                    incoming_generation: snapshot.generation,
                };
            }
        }

        let latest_event_sequence = self.latest_event_sequence(&snapshot.actor_id);
        if snapshot.last_event_sequence < latest_event_sequence {
            return VmPersistentActorStoreOutcome::StaleEvent {
                actor_id: snapshot.actor_id,
                current_sequence: latest_event_sequence,
                incoming_sequence: snapshot.last_event_sequence,
            };
        }

        self.snapshots
            .insert(snapshot.actor_id.clone(), snapshot.clone());
        VmPersistentActorStoreOutcome::SnapshotStored(snapshot)
    }

    fn append_event(&mut self, event: VmPersistentActorEvent) -> VmPersistentActorStoreOutcome {
        if let Some(snapshot) = self.snapshots.get(&event.actor_id) {
            if snapshot.schema != event.schema {
                return Self::incompatible_schema(
                    event.actor_id,
                    snapshot.schema.clone(),
                    event.schema,
                );
            }
            if event.sequence <= snapshot.last_event_sequence {
                return VmPersistentActorStoreOutcome::StaleEvent {
                    actor_id: event.actor_id,
                    current_sequence: snapshot.last_event_sequence,
                    incoming_sequence: event.sequence,
                };
            }
        }

        let events = self.events.entry(event.actor_id.clone()).or_default();
        if let Some(existing) = events.get(&event.sequence) {
            if existing == &event {
                return VmPersistentActorStoreOutcome::Replayed(existing.clone());
            }
            return VmPersistentActorStoreOutcome::DuplicateEvent {
                actor_id: event.actor_id,
                sequence: event.sequence,
            };
        }
        let latest_sequence = events.keys().next_back().copied().unwrap_or(0);
        if event.sequence <= latest_sequence {
            return VmPersistentActorStoreOutcome::StaleEvent {
                actor_id: event.actor_id,
                current_sequence: latest_sequence,
                incoming_sequence: event.sequence,
            };
        }

        events.insert(event.sequence, event.clone());
        VmPersistentActorStoreOutcome::EventAppended(event)
    }

    fn reject_partial_event(
        &mut self,
        event: VmPersistentActorEvent,
    ) -> VmPersistentActorStoreOutcome {
        VmPersistentActorStoreOutcome::PartialWriteRejected {
            actor_id: event.actor_id,
            sequence: event.sequence,
        }
    }

    fn load_snapshot(&self, actor_id: &VmPersistentActorId) -> Option<&VmPersistentActorSnapshot> {
        self.snapshots.get(actor_id)
    }

    fn events_after(
        &self,
        actor_id: &VmPersistentActorId,
        sequence: u64,
    ) -> Vec<VmPersistentActorEvent> {
        self.events
            .get(actor_id)
            .into_iter()
            .flat_map(|events| {
                events
                    .range((sequence + 1)..)
                    .map(|(_, event)| event.clone())
            })
            .collect()
    }

    fn replay(
        &self,
        actor_id: &VmPersistentActorId,
        expected_schema: &VmPersistentActorSchema,
    ) -> Result<VmPersistentActorReplay, VmPersistentActorStoreOutcome> {
        let Some(snapshot) = self.load_snapshot(actor_id) else {
            return Err(VmPersistentActorStoreOutcome::MissingSnapshot(
                actor_id.clone(),
            ));
        };
        if &snapshot.schema != expected_schema {
            return Err(Self::incompatible_schema(
                actor_id.clone(),
                expected_schema.clone(),
                snapshot.schema.clone(),
            ));
        }
        Ok(VmPersistentActorReplay {
            snapshot: snapshot.clone(),
            events: self.events_after(actor_id, snapshot.last_event_sequence),
        })
    }
}

/// File-backed adapter that persists the same typed snapshot/event contract.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmFileBackedPersistentActorStore {
    path: PathBuf,
    inner: VmInMemoryPersistentActorStore,
}

impl VmFileBackedPersistentActorStore {
    pub(crate) fn open_file_backed(path: impl Into<PathBuf>) -> Result<Self, String> {
        let path = path.into();
        let mut store = Self {
            path,
            inner: VmInMemoryPersistentActorStore::new(),
        };
        store.load_file_backed_log()?;
        Ok(store)
    }

    fn load_file_backed_log(&mut self) -> Result<(), String> {
        if !self.path.exists() {
            return Ok(());
        }

        let text = fs::read_to_string(&self.path).map_err(|err| {
            format!(
                "error[vm_persistent_actor]: failed to read file-backed store `{}`: {err}",
                self.path.display()
            )
        })?;
        for (line_index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record = parse_file_backed_record(line).map_err(|err| {
                format!(
                    "error[vm_persistent_actor]: persistent actor file-backed log is corrupt at line {}: {err}",
                    line_index + 1
                )
            })?;
            match record {
                FileBackedRecord::Snapshot(snapshot) => match self.inner.store_snapshot(snapshot) {
                    VmPersistentActorStoreOutcome::SnapshotStored(_) => {}
                    outcome => {
                        return Err(format!(
                            "error[vm_persistent_actor]: persistent actor file-backed log replay rejected snapshot at line {}: {outcome:?}",
                            line_index + 1
                        ));
                    }
                },
                FileBackedRecord::Event(event) => match self.inner.append_event(event) {
                    VmPersistentActorStoreOutcome::EventAppended(_)
                    | VmPersistentActorStoreOutcome::Replayed(_) => {}
                    outcome => {
                        return Err(format!(
                            "error[vm_persistent_actor]: persistent actor file-backed log replay rejected event at line {}: {outcome:?}",
                            line_index + 1
                        ));
                    }
                },
            }
        }
        Ok(())
    }

    fn persist_file_backed_log(&self) -> Result<(), String> {
        let mut lines = Vec::new();
        for snapshot in self.inner.snapshots.values() {
            lines.push(encode_snapshot_record(snapshot)?);
        }
        for events in self.inner.events.values() {
            for event in events.values() {
                lines.push(encode_event_record(event)?);
            }
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "error[vm_persistent_actor]: failed to create file-backed store directory `{}`: {err}",
                    parent.display()
                )
            })?;
        }
        let tmp_path = temporary_file_backed_path(&self.path);
        fs::write(&tmp_path, format!("{}\n", lines.join("\n"))).map_err(|err| {
            format!(
                "error[vm_persistent_actor]: failed to write file-backed store `{}`: {err}",
                tmp_path.display()
            )
        })?;
        fs::rename(&tmp_path, &self.path).map_err(|err| {
            format!(
                "error[vm_persistent_actor]: failed to commit file-backed store `{}`: {err}",
                self.path.display()
            )
        })
    }
}

impl VmPersistentActorStoreAdapter for VmFileBackedPersistentActorStore {
    fn store_snapshot(
        &mut self,
        snapshot: VmPersistentActorSnapshot,
    ) -> VmPersistentActorStoreOutcome {
        if let Err(reason) = encode_snapshot_record(&snapshot) {
            return VmPersistentActorStoreOutcome::PersistenceFailed {
                actor_id: snapshot.actor_id,
                reason,
            };
        }
        let outcome = self.inner.store_snapshot(snapshot);
        if let VmPersistentActorStoreOutcome::SnapshotStored(stored) = &outcome {
            if let Err(reason) = self.persist_file_backed_log() {
                return VmPersistentActorStoreOutcome::PersistenceFailed {
                    actor_id: stored.actor_id.clone(),
                    reason,
                };
            }
        }
        outcome
    }

    fn append_event(&mut self, event: VmPersistentActorEvent) -> VmPersistentActorStoreOutcome {
        if let Err(reason) = encode_event_record(&event) {
            return VmPersistentActorStoreOutcome::PersistenceFailed {
                actor_id: event.actor_id,
                reason,
            };
        }
        let outcome = self.inner.append_event(event);
        if let VmPersistentActorStoreOutcome::EventAppended(appended) = &outcome {
            if let Err(reason) = self.persist_file_backed_log() {
                return VmPersistentActorStoreOutcome::PersistenceFailed {
                    actor_id: appended.actor_id.clone(),
                    reason,
                };
            }
        }
        outcome
    }

    fn reject_partial_event(
        &mut self,
        event: VmPersistentActorEvent,
    ) -> VmPersistentActorStoreOutcome {
        self.inner.reject_partial_event(event)
    }

    fn load_snapshot(&self, actor_id: &VmPersistentActorId) -> Option<&VmPersistentActorSnapshot> {
        self.inner.load_snapshot(actor_id)
    }

    fn events_after(
        &self,
        actor_id: &VmPersistentActorId,
        sequence: u64,
    ) -> Vec<VmPersistentActorEvent> {
        self.inner.events_after(actor_id, sequence)
    }

    fn replay(
        &self,
        actor_id: &VmPersistentActorId,
        expected_schema: &VmPersistentActorSchema,
    ) -> Result<VmPersistentActorReplay, VmPersistentActorStoreOutcome> {
        self.inner.replay(actor_id, expected_schema)
    }
}

/// Embedded key/value adapter using deterministic VM-owned keys and values.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct VmEmbeddedKeyValuePersistentActorStore {
    key_values: BTreeMap<String, String>,
    inner: VmInMemoryPersistentActorStore,
}

impl VmEmbeddedKeyValuePersistentActorStore {
    pub(crate) fn new_embedded_key_value() -> Self {
        Self::default()
    }

    pub(crate) fn from_embedded_key_values(
        key_values: BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let mut store = Self {
            key_values,
            inner: VmInMemoryPersistentActorStore::new(),
        };
        store.load_embedded_key_values()?;
        Ok(store)
    }

    pub(crate) fn export_key_values(&self) -> BTreeMap<String, String> {
        self.key_values.clone()
    }

    fn load_embedded_key_values(&mut self) -> Result<(), String> {
        let mut snapshots = Vec::new();
        let mut events = Vec::new();
        for (key, value) in &self.key_values {
            match parse_embedded_key_value_record(key, value).map_err(|err| {
                format!(
                    "error[vm_persistent_actor]: persistent actor embedded key/value store is corrupt at key `{key}`: {err}"
                )
            })? {
                FileBackedRecord::Snapshot(snapshot) => snapshots.push(snapshot),
                FileBackedRecord::Event(event) => events.push(event),
            }
        }

        for snapshot in snapshots {
            match self.inner.store_snapshot(snapshot) {
                VmPersistentActorStoreOutcome::SnapshotStored(_) => {}
                outcome => {
                    return Err(format!(
                        "error[vm_persistent_actor]: persistent actor embedded key/value replay rejected snapshot: {outcome:?}"
                    ));
                }
            }
        }
        for event in events {
            match self.inner.append_event(event) {
                VmPersistentActorStoreOutcome::EventAppended(_)
                | VmPersistentActorStoreOutcome::Replayed(_) => {}
                outcome => {
                    return Err(format!(
                        "error[vm_persistent_actor]: persistent actor embedded key/value replay rejected event: {outcome:?}"
                    ));
                }
            }
        }
        Ok(())
    }
}

impl VmPersistentActorStoreAdapter for VmEmbeddedKeyValuePersistentActorStore {
    fn store_snapshot(
        &mut self,
        snapshot: VmPersistentActorSnapshot,
    ) -> VmPersistentActorStoreOutcome {
        let key = embedded_snapshot_key(&snapshot.actor_id);
        let value = match encode_snapshot_record(&snapshot) {
            Ok(value) => value,
            Err(reason) => {
                return VmPersistentActorStoreOutcome::PersistenceFailed {
                    actor_id: snapshot.actor_id,
                    reason,
                };
            }
        };
        let outcome = self.inner.store_snapshot(snapshot);
        if matches!(outcome, VmPersistentActorStoreOutcome::SnapshotStored(_)) {
            self.key_values.insert(key, value);
        }
        outcome
    }

    fn append_event(&mut self, event: VmPersistentActorEvent) -> VmPersistentActorStoreOutcome {
        let key = embedded_event_key(&event.actor_id, event.sequence);
        let value = match encode_event_record(&event) {
            Ok(value) => value,
            Err(reason) => {
                return VmPersistentActorStoreOutcome::PersistenceFailed {
                    actor_id: event.actor_id,
                    reason,
                };
            }
        };
        let outcome = self.inner.append_event(event);
        if matches!(outcome, VmPersistentActorStoreOutcome::EventAppended(_)) {
            self.key_values.insert(key, value);
        }
        outcome
    }

    fn reject_partial_event(
        &mut self,
        event: VmPersistentActorEvent,
    ) -> VmPersistentActorStoreOutcome {
        self.inner.reject_partial_event(event)
    }

    fn load_snapshot(&self, actor_id: &VmPersistentActorId) -> Option<&VmPersistentActorSnapshot> {
        self.inner.load_snapshot(actor_id)
    }

    fn events_after(
        &self,
        actor_id: &VmPersistentActorId,
        sequence: u64,
    ) -> Vec<VmPersistentActorEvent> {
        self.inner.events_after(actor_id, sequence)
    }

    fn replay(
        &self,
        actor_id: &VmPersistentActorId,
        expected_schema: &VmPersistentActorSchema,
    ) -> Result<VmPersistentActorReplay, VmPersistentActorStoreOutcome> {
        self.inner.replay(actor_id, expected_schema)
    }
}

/// Database-backed adapter using deterministic SQL row keys and typed records.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmDatabaseBackedPersistentActorStore {
    table_name: String,
    database_rows: BTreeMap<String, String>,
    inner: VmInMemoryPersistentActorStore,
}

impl VmDatabaseBackedPersistentActorStore {
    pub(crate) fn new_database_backed(table_name: impl Into<String>) -> Result<Self, String> {
        let table_name = table_name.into();
        validate_database_table_name(&table_name)?;
        Ok(Self {
            table_name,
            database_rows: BTreeMap::new(),
            inner: VmInMemoryPersistentActorStore::new(),
        })
    }

    pub(crate) fn from_database_rows(
        table_name: impl Into<String>,
        database_rows: BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let table_name = table_name.into();
        validate_database_table_name(&table_name)?;
        let mut store = Self {
            table_name,
            database_rows,
            inner: VmInMemoryPersistentActorStore::new(),
        };
        store.load_database_rows()?;
        Ok(store)
    }

    pub(crate) fn export_database_rows(&self) -> BTreeMap<String, String> {
        self.database_rows.clone()
    }

    pub(crate) fn database_backed_sql_statements(&self) -> Vec<String> {
        DATABASE_BACKED_SQL_STATEMENTS
            .iter()
            .map(|statement| statement.replace("{table}", &self.table_name))
            .collect()
    }

    fn load_database_rows(&mut self) -> Result<(), String> {
        let mut snapshots = Vec::new();
        let mut events = Vec::new();
        for (key, value) in &self.database_rows {
            match parse_database_backed_row_record(key, value).map_err(|err| {
                format!(
                    "error[vm_persistent_actor]: persistent actor database-backed row is corrupt at key `{key}`: {err}"
                )
            })? {
                FileBackedRecord::Snapshot(snapshot) => snapshots.push(snapshot),
                FileBackedRecord::Event(event) => events.push(event),
            }
        }

        for snapshot in snapshots {
            match self.inner.store_snapshot(snapshot) {
                VmPersistentActorStoreOutcome::SnapshotStored(_) => {}
                outcome => {
                    return Err(format!(
                        "error[vm_persistent_actor]: persistent actor database-backed replay rejected snapshot: {outcome:?}"
                    ));
                }
            }
        }
        for event in events {
            match self.inner.append_event(event) {
                VmPersistentActorStoreOutcome::EventAppended(_)
                | VmPersistentActorStoreOutcome::Replayed(_) => {}
                outcome => {
                    return Err(format!(
                        "error[vm_persistent_actor]: persistent actor database-backed replay rejected event: {outcome:?}"
                    ));
                }
            }
        }
        Ok(())
    }
}

impl VmPersistentActorStoreAdapter for VmDatabaseBackedPersistentActorStore {
    fn store_snapshot(
        &mut self,
        snapshot: VmPersistentActorSnapshot,
    ) -> VmPersistentActorStoreOutcome {
        let key = database_snapshot_row_key(&snapshot.actor_id);
        let value = match encode_snapshot_record(&snapshot) {
            Ok(value) => value,
            Err(reason) => {
                return VmPersistentActorStoreOutcome::PersistenceFailed {
                    actor_id: snapshot.actor_id,
                    reason,
                };
            }
        };
        let outcome = self.inner.store_snapshot(snapshot);
        if matches!(outcome, VmPersistentActorStoreOutcome::SnapshotStored(_)) {
            self.database_rows.insert(key, value);
        }
        outcome
    }

    fn append_event(&mut self, event: VmPersistentActorEvent) -> VmPersistentActorStoreOutcome {
        let key = database_event_row_key(&event.actor_id, event.sequence);
        let value = match encode_event_record(&event) {
            Ok(value) => value,
            Err(reason) => {
                return VmPersistentActorStoreOutcome::PersistenceFailed {
                    actor_id: event.actor_id,
                    reason,
                };
            }
        };
        let outcome = self.inner.append_event(event);
        if matches!(outcome, VmPersistentActorStoreOutcome::EventAppended(_)) {
            self.database_rows.insert(key, value);
        }
        outcome
    }

    fn reject_partial_event(
        &mut self,
        event: VmPersistentActorEvent,
    ) -> VmPersistentActorStoreOutcome {
        self.inner.reject_partial_event(event)
    }

    fn load_snapshot(&self, actor_id: &VmPersistentActorId) -> Option<&VmPersistentActorSnapshot> {
        self.inner.load_snapshot(actor_id)
    }

    fn events_after(
        &self,
        actor_id: &VmPersistentActorId,
        sequence: u64,
    ) -> Vec<VmPersistentActorEvent> {
        self.inner.events_after(actor_id, sequence)
    }

    fn replay(
        &self,
        actor_id: &VmPersistentActorId,
        expected_schema: &VmPersistentActorSchema,
    ) -> Result<VmPersistentActorReplay, VmPersistentActorStoreOutcome> {
        self.inner.replay(actor_id, expected_schema)
    }
}

enum FileBackedRecord {
    Snapshot(VmPersistentActorSnapshot),
    Event(VmPersistentActorEvent),
}
