use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::iovec::{VmIoVector, VmIoVectorLimits};
use super::process::{VmProcessId, VmProcessState, VmProcessTable};
use super::ReplValue;

mod trace;

use trace::VmDriverTraceLog;
pub(crate) use trace::{
    VmDriverTraceClass, VmDriverTraceConfig, VmDriverTraceCursor, VmDriverTraceEventKind,
    VmDriverTraceRead,
};

/// Stable identity for one VM-owned driver instance.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmDriverId(u64);

#[cfg(test)]
impl VmDriverId {
    pub(crate) const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Portable limits and identity for one driver adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmDriverDescriptor {
    pub(crate) name: String,
    pub(crate) queue_capacity_bytes: usize,
    pub(crate) callback_capacity: usize,
    pub(crate) max_command_bytes: usize,
    pub(crate) max_environment_value_bytes: usize,
}

#[cfg(test)]
impl VmDriverDescriptor {
    pub(crate) fn new(
        name: impl Into<String>,
        queue_capacity_bytes: usize,
        callback_capacity: usize,
    ) -> Self {
        Self {
            name: name.into(),
            queue_capacity_bytes,
            callback_capacity,
            max_command_bytes: queue_capacity_bytes,
            max_environment_value_bytes: 1_024,
        }
    }

    pub(crate) fn with_max_command_bytes(mut self, max_command_bytes: usize) -> Self {
        self.max_command_bytes = max_command_bytes;
        self
    }

    pub(crate) fn with_max_environment_value_bytes(mut self, max_bytes: usize) -> Self {
        self.max_environment_value_bytes = max_bytes;
        self
    }
}

/// Placement used by the portable driver byte queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDriverQueuePlacement {
    Front,
    Back,
}

/// One exactly-once callback emitted by a driver helper.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmDriverCallback {
    pub(crate) sequence: u64,
    pub(crate) payload: Vec<u8>,
}

/// One deterministic driver timer completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmDriverTimerEvent {
    pub(crate) driver: VmDriverId,
    pub(crate) controller: VmProcessId,
    pub(crate) deadline_tick: u64,
}

/// Stable inspection row for one live driver.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmDriverSnapshot {
    pub(crate) id: VmDriverId,
    pub(crate) name: String,
    pub(crate) owner: VmProcessId,
    pub(crate) controller: VmProcessId,
    pub(crate) queued_bytes: usize,
    pub(crate) pending_callbacks: usize,
    pub(crate) timer_deadline_tick: Option<u64>,
    pub(crate) environment_entries: usize,
}

/// Resources released by explicit close or owner-exit cleanup.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDriverCloseReport {
    pub(crate) id: VmDriverId,
    pub(crate) owner: VmProcessId,
    pub(crate) controller: VmProcessId,
    pub(crate) released_queue_bytes: usize,
    pub(crate) released_callbacks: usize,
    pub(crate) cancelled_timer: bool,
    pub(crate) released_environment_entries: usize,
}

#[derive(Debug)]
#[cfg(test)]
struct VmDriverRecord {
    id: VmDriverId,
    descriptor: VmDriverDescriptor,
    owner: VmProcessId,
    controller: VmProcessId,
    queue: VecDeque<u8>,
    callbacks: VecDeque<VmDriverCallback>,
    callback_sequences: BTreeSet<u64>,
    timer_deadline_tick: Option<u64>,
    environment: BTreeMap<String, String>,
}

/// VM-owned portable driver adapter runtime.
///
/// The runtime owns driver identity, controllers, bounded byte and callback
/// queues, logical timers, and isolated environment data. Host helpers may
/// produce bytes or readiness, but they cannot own process scheduling, expose
/// raw file descriptors, or mutate the host process environment.
#[derive(Debug, Default)]
#[cfg(test)]
pub(crate) struct VmDriverRuntime {
    next_id: u64,
    current_tick: u64,
    drivers: BTreeMap<VmDriverId, VmDriverRecord>,
    trace: VmDriverTraceLog,
}

#[cfg(test)]
impl VmDriverRuntime {
    /// Replaces the provider-neutral driver trace selection atomically.
    pub(crate) fn configure_trace(&mut self, config: VmDriverTraceConfig) {
        self.trace.configure(config);
    }

    /// Captures the next driver trace sequence for an incremental consumer.
    pub(crate) const fn trace_cursor(&self) -> VmDriverTraceCursor {
        self.trace.cursor()
    }

    /// Returns the oldest sequence still retained by the bounded trace log.
    pub(crate) fn oldest_trace_cursor(&self) -> VmDriverTraceCursor {
        self.trace.oldest_cursor()
    }

    /// Reads immutable driver diagnostics from an exact retained cursor.
    pub(crate) fn trace_since(
        &self,
        cursor: VmDriverTraceCursor,
    ) -> Result<VmDriverTraceRead, String> {
        self.trace.since(cursor)
    }

    /// Opens a driver owned and controlled by one live VM process.
    pub(crate) fn open(
        &mut self,
        processes: &VmProcessTable,
        owner: VmProcessId,
        descriptor: VmDriverDescriptor,
    ) -> Result<VmDriverId, String> {
        ensure_live_process(processes, owner, "driver owner")?;
        validate_descriptor(&descriptor)?;
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or_else(|| "VM driver identity space exhausted".to_string())?;
        let id = VmDriverId(next_id);
        let name = descriptor.name.clone();
        self.drivers.insert(
            id,
            VmDriverRecord {
                id,
                descriptor,
                owner,
                controller: owner,
                queue: VecDeque::new(),
                callbacks: VecDeque::new(),
                callback_sequences: BTreeSet::new(),
                timer_deadline_tick: None,
                environment: BTreeMap::new(),
            },
        );
        self.next_id = next_id;
        self.trace.record(
            self.current_tick,
            id,
            owner,
            owner,
            VmDriverTraceEventKind::Opened { name },
        );
        Ok(id)
    }

    /// Transfers command authority without changing driver ownership.
    pub(crate) fn connect(
        &mut self,
        processes: &VmProcessTable,
        driver: VmDriverId,
        requester: VmProcessId,
        next_controller: VmProcessId,
    ) -> Result<(), String> {
        let (owner, previous) = {
            let record = self.ensure_controller(driver, requester)?;
            (record.owner, record.controller)
        };
        ensure_live_process(processes, next_controller, "next driver controller")?;
        self.drivers
            .get_mut(&driver)
            .expect("driver controller was validated before transfer")
            .controller = next_controller;
        self.trace.record(
            self.current_tick,
            driver,
            owner,
            requester,
            VmDriverTraceEventKind::ControllerChanged {
                previous,
                next: next_controller,
            },
        );
        Ok(())
    }

    /// Flattens one scatter/gather command after complete size validation.
    pub(crate) fn commandv(
        &mut self,
        driver: VmDriverId,
        requester: VmProcessId,
        segments: &[&[u8]],
    ) -> Result<Vec<u8>, String> {
        let (owner, max_command_bytes) = {
            let record = self.ensure_controller(driver, requester)?;
            (record.owner, record.descriptor.max_command_bytes)
        };
        let total = checked_segment_bytes(segments)?;
        if total > max_command_bytes {
            return Err(format!(
                "driver command is {total} bytes; limit is {max_command_bytes}"
            ));
        }
        let mut command = Vec::with_capacity(total);
        for segment in segments {
            command.extend_from_slice(segment);
        }
        self.trace.record(
            self.current_tick,
            driver,
            owner,
            requester,
            VmDriverTraceEventKind::Command {
                segments: segments.len(),
                bytes: total,
            },
        );
        Ok(command)
    }

    /// Normalizes VM iodata before executing one bounded driver command.
    pub(crate) fn command_value(
        &mut self,
        driver: VmDriverId,
        requester: VmProcessId,
        value: &ReplValue,
    ) -> Result<Vec<u8>, String> {
        let (owner, max_command_bytes) = {
            let record = self.ensure_controller(driver, requester)?;
            (record.owner, record.descriptor.max_command_bytes)
        };
        let vector =
            VmIoVector::from_value(value, VmIoVectorLimits::for_byte_limit(max_command_bytes))
                .map_err(|error| format!("invalid driver command: {error}"))?;
        let segments = vector.segment_count();
        let command = vector.flatten();
        self.trace.record(
            self.current_tick,
            driver,
            owner,
            requester,
            VmDriverTraceEventKind::Command {
                segments,
                bytes: command.len(),
            },
        );
        Ok(command)
    }

    /// Adds scatter/gather bytes to the front or back of a bounded queue.
    pub(crate) fn queue(
        &mut self,
        driver: VmDriverId,
        requester: VmProcessId,
        placement: VmDriverQueuePlacement,
        segments: &[&[u8]],
    ) -> Result<usize, String> {
        let (owner, queued_bytes, queue_capacity_bytes) = {
            let record = self.ensure_controller(driver, requester)?;
            (
                record.owner,
                record.queue.len(),
                record.descriptor.queue_capacity_bytes,
            )
        };
        let added = checked_segment_bytes(segments)?;
        let projected = queued_bytes
            .checked_add(added)
            .ok_or_else(|| "driver queue byte count overflow".to_string())?;
        if projected > queue_capacity_bytes {
            return Err(format!(
                "driver queue would contain {projected} bytes; capacity is {queue_capacity_bytes}"
            ));
        }
        let bytes = flatten_segments(segments, added);
        let record = self
            .drivers
            .get_mut(&driver)
            .expect("driver was validated before queue mutation");
        match placement {
            VmDriverQueuePlacement::Front => {
                for byte in bytes.into_iter().rev() {
                    record.queue.push_front(byte);
                }
            }
            VmDriverQueuePlacement::Back => record.queue.extend(bytes),
        }
        let queued_bytes = record.queue.len();
        self.trace.record(
            self.current_tick,
            driver,
            owner,
            requester,
            VmDriverTraceEventKind::Queued {
                placement,
                bytes: added,
                queued_bytes,
            },
        );
        Ok(queued_bytes)
    }

    pub(crate) fn bytes_queued(
        &self,
        driver: VmDriverId,
        requester: VmProcessId,
    ) -> Result<usize, String> {
        Ok(self.ensure_controller(driver, requester)?.queue.len())
    }

    /// Copies, but does not consume, up to `size` bytes from the queue head.
    pub(crate) fn read_head(
        &self,
        driver: VmDriverId,
        requester: VmProcessId,
        size: usize,
    ) -> Result<Vec<u8>, String> {
        Ok(self
            .ensure_controller(driver, requester)?
            .queue
            .iter()
            .take(size)
            .copied()
            .collect())
    }

    /// Atomically consumes an exact number of queued bytes.
    pub(crate) fn dequeue(
        &mut self,
        driver: VmDriverId,
        requester: VmProcessId,
        size: usize,
    ) -> Result<Vec<u8>, String> {
        let (owner, queued_bytes) = {
            let record = self.ensure_controller(driver, requester)?;
            (record.owner, record.queue.len())
        };
        if size > queued_bytes {
            return Err(format!(
                "cannot dequeue {size} bytes from {queued_bytes} queued bytes"
            ));
        }
        let record = self
            .drivers
            .get_mut(&driver)
            .expect("driver was validated before dequeue");
        let bytes = record.queue.drain(..size).collect();
        let queued_bytes = record.queue.len();
        self.trace.record(
            self.current_tick,
            driver,
            owner,
            requester,
            VmDriverTraceEventKind::Dequeued {
                bytes: size,
                queued_bytes,
            },
        );
        Ok(bytes)
    }

    /// Starts or replaces the driver's logical timer.
    pub(crate) fn set_timer(
        &mut self,
        driver: VmDriverId,
        requester: VmProcessId,
        delay_ticks: u64,
    ) -> Result<u64, String> {
        let owner = self.ensure_controller(driver, requester)?.owner;
        let deadline = self
            .current_tick
            .checked_add(delay_ticks)
            .ok_or_else(|| "driver timer deadline overflow".to_string())?;
        self.drivers
            .get_mut(&driver)
            .expect("driver was validated before timer mutation")
            .timer_deadline_tick = Some(deadline);
        self.trace.record(
            self.current_tick,
            driver,
            owner,
            requester,
            VmDriverTraceEventKind::TimerSet {
                deadline_tick: deadline,
            },
        );
        Ok(deadline)
    }

    /// Cancels an active timer, returning false when no timer was pending.
    pub(crate) fn cancel_timer(
        &mut self,
        driver: VmDriverId,
        requester: VmProcessId,
    ) -> Result<bool, String> {
        let owner = self.ensure_controller(driver, requester)?.owner;
        let was_pending = self
            .drivers
            .get_mut(&driver)
            .expect("driver was validated before timer cancellation")
            .timer_deadline_tick
            .take()
            .is_some();
        self.trace.record(
            self.current_tick,
            driver,
            owner,
            requester,
            VmDriverTraceEventKind::TimerCancelled { was_pending },
        );
        Ok(was_pending)
    }

    /// Advances the deterministic clock and fires each due timer once.
    pub(crate) fn advance_to(&mut self, tick: u64) -> Result<Vec<VmDriverTimerEvent>, String> {
        if tick < self.current_tick {
            return Err(format!(
                "driver clock cannot move backward from {} to {tick}",
                self.current_tick
            ));
        }
        self.current_tick = tick;
        let due = self
            .drivers
            .iter()
            .filter_map(|(id, record)| {
                record
                    .timer_deadline_tick
                    .filter(|deadline| *deadline <= tick)
                    .map(|deadline| (*id, record.controller, deadline))
            })
            .collect::<Vec<_>>();
        for (driver, controller, deadline_tick) in &due {
            let owner = self
                .drivers
                .get(driver)
                .expect("due driver came from live table")
                .owner;
            self.drivers
                .get_mut(driver)
                .expect("due driver came from live table")
                .timer_deadline_tick = None;
            self.trace.record(
                tick,
                *driver,
                owner,
                *controller,
                VmDriverTraceEventKind::TimerFired {
                    deadline_tick: *deadline_tick,
                },
            );
        }
        Ok(due
            .into_iter()
            .map(|(driver, controller, deadline_tick)| VmDriverTimerEvent {
                driver,
                controller,
                deadline_tick,
            })
            .collect())
    }

    pub(crate) const fn current_tick(&self) -> u64 {
        self.current_tick
    }

    /// Stores adapter-local environment data without changing host globals.
    pub(crate) fn put_environment(
        &mut self,
        driver: VmDriverId,
        requester: VmProcessId,
        key: &str,
        value: &str,
    ) -> Result<(), String> {
        let record = self.ensure_controller(driver, requester)?;
        validate_environment_key(key)?;
        if value.len() > record.descriptor.max_environment_value_bytes {
            return Err(format!(
                "driver environment value is {} bytes; limit is {}",
                value.len(),
                record.descriptor.max_environment_value_bytes
            ));
        }
        self.drivers
            .get_mut(&driver)
            .expect("driver was validated before environment mutation")
            .environment
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    pub(crate) fn environment(
        &self,
        driver: VmDriverId,
        requester: VmProcessId,
        key: &str,
    ) -> Result<Option<&str>, String> {
        validate_environment_key(key)?;
        Ok(self
            .ensure_controller(driver, requester)?
            .environment
            .get(key)
            .map(String::as_str))
    }

    /// Queues one callback exactly once with bounded pending depth.
    pub(crate) fn submit_callback(
        &mut self,
        driver: VmDriverId,
        requester: VmProcessId,
        callback: VmDriverCallback,
    ) -> Result<(), String> {
        let record = self.ensure_controller(driver, requester)?;
        let owner = record.owner;
        if callback.sequence == 0 {
            return Err("driver callback sequence must be nonzero".to_string());
        }
        if callback.payload.len() > record.descriptor.max_command_bytes {
            return Err(format!(
                "driver callback is {} bytes; limit is {}",
                callback.payload.len(),
                record.descriptor.max_command_bytes
            ));
        }
        if record.callback_sequences.contains(&callback.sequence) {
            return Err(format!(
                "driver callback sequence {} is duplicate",
                callback.sequence
            ));
        }
        if record.callbacks.len() >= record.descriptor.callback_capacity {
            return Err(format!(
                "driver callback queue is full at {} entries",
                record.descriptor.callback_capacity
            ));
        }
        let record = self
            .drivers
            .get_mut(&driver)
            .expect("driver was validated before callback mutation");
        let callback_sequence = callback.sequence;
        let bytes = callback.payload.len();
        record.callback_sequences.insert(callback.sequence);
        record.callbacks.push_back(callback);
        self.trace.record(
            self.current_tick,
            driver,
            owner,
            requester,
            VmDriverTraceEventKind::CallbackSubmitted {
                callback_sequence,
                bytes,
            },
        );
        Ok(())
    }

    /// Drains callbacks in submission order.
    pub(crate) fn drain_callbacks(
        &mut self,
        driver: VmDriverId,
        requester: VmProcessId,
        limit: usize,
    ) -> Result<Vec<VmDriverCallback>, String> {
        let owner = self.ensure_controller(driver, requester)?.owner;
        if limit == 0 {
            return Err("driver callback drain limit must be greater than 0".to_string());
        }
        let record = self
            .drivers
            .get_mut(&driver)
            .expect("driver was validated before callback drain");
        let count = limit.min(record.callbacks.len());
        let callbacks = record.callbacks.drain(..count).collect();
        self.trace.record(
            self.current_tick,
            driver,
            owner,
            requester,
            VmDriverTraceEventKind::CallbacksDrained { count },
        );
        Ok(callbacks)
    }

    /// Closes a driver through its current controller.
    pub(crate) fn close(
        &mut self,
        driver: VmDriverId,
        requester: VmProcessId,
    ) -> Result<VmDriverCloseReport, String> {
        self.ensure_controller(driver, requester)?;
        let record = self
            .drivers
            .remove(&driver)
            .expect("driver was validated before close");
        let report = close_report(record);
        self.trace
            .record_close(self.current_tick, requester, false, &report);
        Ok(report)
    }

    /// Releases all drivers owned or controlled by an exited process.
    pub(crate) fn cleanup_process(&mut self, process: VmProcessId) -> Vec<VmDriverCloseReport> {
        let ids = self
            .drivers
            .iter()
            .filter_map(|(id, record)| {
                (record.owner == process || record.controller == process).then_some(*id)
            })
            .collect::<Vec<_>>();
        let reports = ids
            .into_iter()
            .map(|id| {
                close_report(
                    self.drivers
                        .remove(&id)
                        .expect("cleanup id came from live driver table"),
                )
            })
            .collect::<Vec<_>>();
        for report in &reports {
            self.trace
                .record_close(self.current_tick, process, true, report);
        }
        reports
    }

    pub(crate) fn snapshots(&self) -> Vec<VmDriverSnapshot> {
        self.drivers.values().map(snapshot).collect()
    }

    pub(crate) fn snapshot(&self, driver: VmDriverId) -> Result<VmDriverSnapshot, String> {
        self.drivers
            .get(&driver)
            .map(snapshot)
            .ok_or_else(|| format!("driver {} is not open", driver.as_u64()))
    }

    fn ensure_controller(
        &self,
        driver: VmDriverId,
        requester: VmProcessId,
    ) -> Result<&VmDriverRecord, String> {
        let record = self
            .drivers
            .get(&driver)
            .ok_or_else(|| format!("driver {} is not open", driver.as_u64()))?;
        if record.controller != requester {
            return Err(format!(
                "driver {} is controlled by process {}, not {}",
                driver.as_u64(),
                record.controller.as_u64(),
                requester.as_u64()
            ));
        }
        Ok(record)
    }
}

#[cfg(test)]
fn validate_descriptor(descriptor: &VmDriverDescriptor) -> Result<(), String> {
    if descriptor.name.trim().is_empty() {
        return Err("VM driver name cannot be empty".to_string());
    }
    if descriptor.queue_capacity_bytes == 0 {
        return Err("VM driver queue capacity must be greater than 0".to_string());
    }
    if descriptor.callback_capacity == 0 {
        return Err("VM driver callback capacity must be greater than 0".to_string());
    }
    if descriptor.max_command_bytes == 0 {
        return Err("VM driver command limit must be greater than 0".to_string());
    }
    if descriptor.max_environment_value_bytes == 0 {
        return Err("VM driver environment limit must be greater than 0".to_string());
    }
    Ok(())
}

#[cfg(test)]
fn validate_environment_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("driver environment key cannot be empty".to_string());
    }
    if key.as_bytes().contains(&0) {
        return Err("driver environment key cannot contain NUL".to_string());
    }
    Ok(())
}

#[cfg(test)]
fn checked_segment_bytes(segments: &[&[u8]]) -> Result<usize, String> {
    segments.iter().try_fold(0usize, |total, segment| {
        total
            .checked_add(segment.len())
            .ok_or_else(|| "driver scatter/gather byte count overflow".to_string())
    })
}

#[cfg(test)]
fn flatten_segments(segments: &[&[u8]], total: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(total);
    for segment in segments {
        bytes.extend_from_slice(segment);
    }
    bytes
}

#[cfg(test)]
fn snapshot(record: &VmDriverRecord) -> VmDriverSnapshot {
    VmDriverSnapshot {
        id: record.id,
        name: record.descriptor.name.clone(),
        owner: record.owner,
        controller: record.controller,
        queued_bytes: record.queue.len(),
        pending_callbacks: record.callbacks.len(),
        timer_deadline_tick: record.timer_deadline_tick,
        environment_entries: record.environment.len(),
    }
}

#[cfg(test)]
fn close_report(record: VmDriverRecord) -> VmDriverCloseReport {
    VmDriverCloseReport {
        id: record.id,
        owner: record.owner,
        controller: record.controller,
        released_queue_bytes: record.queue.len(),
        released_callbacks: record.callbacks.len(),
        cancelled_timer: record.timer_deadline_tick.is_some(),
        released_environment_entries: record.environment.len(),
    }
}

#[cfg(test)]
fn ensure_live_process(
    processes: &VmProcessTable,
    process: VmProcessId,
    role: &str,
) -> Result<(), String> {
    let record = processes
        .get(process)
        .ok_or_else(|| format!("{role} process {} does not exist", process.as_u64()))?;
    if matches!(record.state, VmProcessState::Exited(_)) {
        return Err(format!("{role} process {} has exited", process.as_u64()));
    }
    Ok(())
}

#[cfg(test)]
#[path = "driver_beam_suite_parity_test.rs"]
#[cfg(test)]
mod driver_beam_suite_parity_test;

#[cfg(test)]
#[path = "driver/lttng_beam_suite_parity_test.rs"]
#[cfg(test)]
mod lttng_beam_suite_parity_test;
