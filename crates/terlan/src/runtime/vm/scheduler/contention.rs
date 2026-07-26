//! Owner-local, category-filtered VM contention telemetry.

use std::collections::{BTreeMap, BTreeSet};

const VM_CONTENTION_RECORD_CAPACITY: usize = 4_096;

/// VM-owned contention domains. These describe semantic runtime resources,
/// never Rust mutex implementations or operating-system lock addresses.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub(crate) enum VmContentionCategory {
    Memory,
    Table,
    Diagnostics,
    Distribution,
    Runtime,
    Io,
    Process,
    Scheduler,
}

impl VmContentionCategory {
    pub(crate) const ALL: [Self; 8] = [
        Self::Memory,
        Self::Table,
        Self::Diagnostics,
        Self::Distribution,
        Self::Runtime,
        Self::Io,
        Self::Process,
        Self::Scheduler,
    ];

    pub(crate) const fn control_name(self) -> &'static str {
        match self {
            Self::Memory => "allocator",
            Self::Table => "db",
            Self::Diagnostics => "debug",
            Self::Distribution => "distribution",
            Self::Runtime => "generic",
            Self::Io => "io",
            Self::Process => "process",
            Self::Scheduler => "scheduler",
        }
    }

    fn parse(value: &str) -> Result<Self, VmContentionError> {
        Self::ALL
            .into_iter()
            .find(|category| category.control_name() == value)
            .ok_or_else(|| VmContentionError::InvalidCategory(value.to_string()))
    }

    const fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// Stable control and resource validation failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmContentionError {
    InvalidCategory(String),
    EmptyIdentity,
    EmptyLabel,
    CapacityExceeded {
        capacity: usize,
    },
    IdentityCollision {
        category: VmContentionCategory,
        identity: String,
    },
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct VmContentionResourceKey {
    category: VmContentionCategory,
    identity: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VmContentionRecord {
    label: String,
    active: bool,
    acquisitions: u64,
    contentions: u64,
    total_wait_ticks: u64,
    max_wait_ticks: u64,
}

/// Immutable resource row returned to diagnostics and tooling.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmContentionRecordSnapshot {
    pub(crate) category: VmContentionCategory,
    pub(crate) identity: String,
    pub(crate) label: String,
    pub(crate) active: bool,
    pub(crate) acquisitions: u64,
    pub(crate) contentions: u64,
    pub(crate) total_wait_ticks: u64,
    pub(crate) max_wait_ticks: u64,
}

/// Deterministically ordered contention snapshot.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmContentionSnapshot {
    pub(crate) records: Vec<VmContentionRecordSnapshot>,
    pub(crate) dropped_records: u64,
}

/// Mutable telemetry owned by exactly one scheduler/shard thread.
///
/// Disabled categories return before allocating an identity or touching the
/// resource map. Consequently ordinary scheduler execution pays only one
/// integer bit test and never acquires a telemetry lock.
#[derive(Debug)]
pub(crate) struct VmContentionTelemetry {
    enabled_mask: u8,
    retain_retired: bool,
    max_records: usize,
    dropped_records: u64,
    records: BTreeMap<VmContentionResourceKey, VmContentionRecord>,
}

impl Default for VmContentionTelemetry {
    fn default() -> Self {
        Self {
            enabled_mask: 0,
            retain_retired: false,
            max_records: VM_CONTENTION_RECORD_CAPACITY,
            dropped_records: 0,
            records: BTreeMap::new(),
        }
    }
}

impl VmContentionTelemetry {
    #[cfg(test)]
    pub(crate) fn with_capacity(max_records: usize) -> Self {
        Self {
            max_records,
            ..Self::default()
        }
    }

    /// Replaces the enabled mask only after every category has been validated.
    pub(crate) fn configure(&mut self, categories: &[&str]) -> Result<(), VmContentionError> {
        let mut enabled_mask = 0u8;
        for category in categories {
            enabled_mask |= VmContentionCategory::parse(category)?.bit();
        }
        self.enabled_mask = enabled_mask;
        Ok(())
    }

    pub(crate) fn enabled_categories(&self) -> Vec<VmContentionCategory> {
        VmContentionCategory::ALL
            .into_iter()
            .filter(|category| self.is_enabled(*category))
            .collect()
    }

    pub(crate) fn set_retain_retired(&mut self, retain: bool) {
        self.retain_retired = retain;
    }

    pub(crate) const fn retain_retired(&self) -> bool {
        self.retain_retired
    }

    /// Registers one semantic runtime resource without recording contention.
    pub(crate) fn register_resource(
        &mut self,
        category: VmContentionCategory,
        identity: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<(), VmContentionError> {
        let identity = identity.into();
        let label = label.into();
        if identity.trim().is_empty() {
            return Err(VmContentionError::EmptyIdentity);
        }
        if label.trim().is_empty() {
            return Err(VmContentionError::EmptyLabel);
        }
        let key = VmContentionResourceKey { category, identity };
        match self.records.get_mut(&key) {
            Some(record) if record.label == label => {
                record.active = true;
                Ok(())
            }
            Some(_) => Err(VmContentionError::IdentityCollision {
                category,
                identity: key.identity,
            }),
            None => {
                if self.records.len() >= self.max_records {
                    return Err(VmContentionError::CapacityExceeded {
                        capacity: self.max_records,
                    });
                }
                self.records.insert(
                    key,
                    VmContentionRecord {
                        label,
                        active: true,
                        acquisitions: 0,
                        contentions: 0,
                        total_wait_ticks: 0,
                        max_wait_ticks: 0,
                    },
                );
                Ok(())
            }
        }
    }

    /// Retires one resource, optionally retaining its final counters.
    pub(crate) fn retire_resource(&mut self, category: VmContentionCategory, identity: &str) {
        let key = VmContentionResourceKey {
            category,
            identity: identity.to_string(),
        };
        if self.retain_retired {
            if let Some(record) = self.records.get_mut(&key) {
                record.active = false;
            }
        } else {
            self.records.remove(&key);
        }
    }

    /// Synchronizes stable registered actor and table names into the catalog.
    pub(crate) fn synchronize_registered_resources<P, T>(
        &mut self,
        processes: P,
        tables: T,
    ) -> Result<(), VmContentionError>
    where
        P: IntoIterator<Item = (String, String)>,
        T: IntoIterator<Item = (String, String)>,
    {
        let mut live_processes = BTreeSet::new();
        for (identity, label) in processes {
            self.register_resource(VmContentionCategory::Process, identity.clone(), label)?;
            live_processes.insert(identity);
        }
        self.retire_missing(VmContentionCategory::Process, &live_processes);

        let mut live_tables = BTreeSet::new();
        for (identity, label) in tables {
            self.register_resource(VmContentionCategory::Table, identity.clone(), label)?;
            live_tables.insert(identity);
        }
        self.retire_missing(VmContentionCategory::Table, &live_tables);
        Ok(())
    }

    /// Clears counters and discarded retired rows without changing the mask.
    pub(crate) fn clear(&mut self) {
        self.records.retain(|_, record| record.active);
        self.dropped_records = 0;
        for record in self.records.values_mut() {
            record.acquisitions = 0;
            record.contentions = 0;
            record.total_wait_ticks = 0;
            record.max_wait_ticks = 0;
        }
    }

    /// Returns only resources in currently enabled categories.
    pub(crate) fn snapshot(&self) -> VmContentionSnapshot {
        VmContentionSnapshot {
            records: self
                .records
                .iter()
                .filter(|(key, _)| self.is_enabled(key.category))
                .map(|(key, record)| VmContentionRecordSnapshot {
                    category: key.category,
                    identity: key.identity.clone(),
                    label: record.label.clone(),
                    active: record.active,
                    acquisitions: record.acquisitions,
                    contentions: record.contentions,
                    total_wait_ticks: record.total_wait_ticks,
                    max_wait_ticks: record.max_wait_ticks,
                })
                .collect(),
            dropped_records: self.dropped_records,
        }
    }

    /// Records scheduler queue delay without allocating while disabled.
    pub(super) fn observe_scheduler_wait(&mut self, pid: u64, wait_ticks: u64) {
        if !self.is_enabled(VmContentionCategory::Scheduler) {
            return;
        }
        let identity = format!("process:{pid}");
        let key = VmContentionResourceKey {
            category: VmContentionCategory::Scheduler,
            identity,
        };
        if !self.records.contains_key(&key) && self.records.len() >= self.max_records {
            self.dropped_records = self.dropped_records.saturating_add(1);
            return;
        }
        let record = self
            .records
            .entry(key)
            .or_insert_with(|| VmContentionRecord {
                label: format!("process-{pid}"),
                active: true,
                acquisitions: 0,
                contentions: 0,
                total_wait_ticks: 0,
                max_wait_ticks: 0,
            });
        record.acquisitions = record.acquisitions.saturating_add(1);
        if wait_ticks > 0 {
            record.contentions = record.contentions.saturating_add(1);
            record.total_wait_ticks = record.total_wait_ticks.saturating_add(wait_ticks);
            record.max_wait_ticks = record.max_wait_ticks.max(wait_ticks);
        }
    }

    fn retire_missing(&mut self, category: VmContentionCategory, live: &BTreeSet<String>) {
        let missing = self
            .records
            .keys()
            .filter(|key| key.category == category && !live.contains(&key.identity))
            .map(|key| key.identity.clone())
            .collect::<Vec<_>>();
        for identity in missing {
            self.retire_resource(category, &identity);
        }
    }

    const fn is_enabled(&self, category: VmContentionCategory) -> bool {
        self.enabled_mask & category.bit() != 0
    }
}

#[cfg(test)]
#[path = "contention_beam_suite_parity_test.rs"]
mod contention_beam_suite_parity_test;
