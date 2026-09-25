//! Supervisor-selected durable workers and nonblocking, fixed-owner completions.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::task::Waker;
use std::time::{Duration, Instant};

use super::{distributed_storage, VmRuntimeError, VmRuntimeResult};
use crate::runtime::vm::capability_worker::*;
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::{PureNativeCapabilityWait, PureNativeSuspension};
use crate::terlan_native_boundary::metadata::NativeBoundaryExecutionProfile;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

/// Selects only the installed sibling worker; PATH and application input grant no authority.
fn installed_worker_path(executable: &Path) -> VmRuntimeResult<PathBuf> {
    if !executable.is_absolute() || executable.file_name().is_none() {
        return Err(
            "error[vm.distributed_storage.executable]: expected an absolute executable path".into(),
        );
    }
    let directory = executable
        .parent()
        .ok_or("error[vm.distributed_storage.executable]: missing executable directory")?;
    Ok(directory.join(if cfg!(windows) {
        "terlan-native-worker.exe"
    } else {
        "terlan-native-worker"
    }))
}

/// Explicit host authority, never constructed from an application's storage policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmStorageBinding {
    /// Stable source-visible name selected by the supervisor.
    pub(super) name: String,
    /// Trusted logical identity retained by the supervisor across VM restarts.
    pub(super) expected_identity: Option<[u8; 32]>,
    directory: PathBuf,
}

impl VmStorageBinding {
    /// Parses a canonical backend name and an absolute supervisor-selected directory.
    pub(crate) fn parse(value: &str) -> VmRuntimeResult<Self> {
        let (name, directory) = value
            .split_once('=')
            .ok_or("error[vm.distributed_storage.binding]: expected NAME=/absolute/directory")?;
        let (name, expected_identity) = match name.split_once('@') {
            Some((name, identity)) => (name, Some(parse_identity(identity)?)),
            None => (name, None),
        };
        if name.is_empty()
            || name.len() > 64
            || !name.as_bytes()[0].is_ascii_lowercase()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
            || !Path::new(directory).is_absolute()
        {
            return Err("error[vm.distributed_storage.binding]: expected canonical lowercase name and absolute directory".into());
        }
        Ok(Self {
            name: name.into(),
            expected_identity,
            directory: directory.into(),
        })
    }
}

/// Rejects ambiguous encodings and uninitialized identities before spawning workers.
fn parse_identity(value: &str) -> VmRuntimeResult<[u8; 32]> {
    let invalid = "error[vm.distributed_storage.binding]: expected 64 lowercase hexadecimal digits for a nonzero storage identity";
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid.into());
    }
    let mut identity = [0; 32];
    for (index, byte) in identity.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).map_err(|_| invalid)?;
    }
    if identity == [0; 32] {
        return Err(invalid.into());
    }
    Ok(identity)
}

/// Continuation authority and result projection stay in the VM, outside the worker.
pub(super) struct StorageWait {
    pub(super) owner: VmProcessId,
    pub(super) suspension: PureNativeSuspension,
    pub(super) wait: PureNativeCapabilityWait,
    pub(super) projection: distributed_storage::Pending,
    pub(super) context: VmCapabilityRequestContext,
    /// All probes of a source call share this deadline; a continuation cannot renew it.
    pub(super) deadline: Instant,
}

/// One correlated completion, or a terminal failure with explicit recovery semantics.
pub(super) struct StorageCompletion {
    pub(super) pending: StorageWait,
    pub(super) reply: VmRuntimeResult<NativeBoundaryReplyTerm>,
}

struct Backend {
    pump: VmCapabilityWorkerEventPump<StorageWait>,
    deadlines: Vec<(VmCapabilityWorkerParkedRequest, Instant)>,
}

/// Bounded worker queues are polled by the execution owner; SQLite never runs there.
#[derive(Default)]
pub(super) struct VmStorageWorkers {
    backends: BTreeMap<String, Backend>,
    completed: VecDeque<StorageCompletion>,
}

impl VmStorageWorkers {
    /// Resolves transport only for explicit authority, never for ordinary actor execution.
    pub(super) fn start_installed(bindings: &[VmStorageBinding]) -> VmRuntimeResult<Self> {
        if bindings.is_empty() {
            return Ok(Self::default());
        }
        let executable = std::env::current_exe()
            .map_err(|error| format!("error[vm.distributed_storage.executable]: {error}"))?;
        Self::start(bindings, &installed_worker_path(&executable)?)
    }

    /// Returns whether any supervisor-selected backend has been installed.
    pub(super) fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }

    /// Validates the complete binding set before starting any external processes.
    pub(super) fn start(bindings: &[VmStorageBinding], worker: &Path) -> VmRuntimeResult<Self> {
        if bindings.len() > 16 {
            return Err(
                "error[vm.distributed_storage.binding]: at most 16 durable backends are admitted"
                    .into(),
            );
        }
        let mut names = BTreeSet::new();
        let mut directories = BTreeSet::new();
        let mut policies = Vec::new();
        for binding in bindings {
            if !names.insert(binding.name.clone()) {
                return Err("error[vm.distributed_storage.binding]: duplicate backend name".into());
            }
            let directory = binding.directory.canonicalize().map_err(|_| {
                "error[vm.distributed_storage.binding]: durable directory must already exist"
            })?;
            if !directories.insert(directory.clone()) {
                return Err(
                    "error[vm.distributed_storage.binding]: duplicate durable directory".into(),
                );
            }
            let mut policy = VmCapabilityWorkerPolicy::new(
                worker,
                NativeBoundaryExecutionProfile::CrashIsolated,
            )?
            .allow("storage")
            .admit_worker_class("blocking")
            .with_credit_limit(32)?;
            policy.storage_directory = Some(directory);
            policies.push((binding.name.clone(), policy));
        }
        let mut workers = Self::default();
        for (name, policy) in policies {
            let identity = VmCapabilityWorkerIdentity::new(
                VmCapabilityWorkerId::new(format!("storage-{name}"))?,
                VmCapabilityWorkerGeneration::new(1)?,
            );
            let client = VmCapabilityWorkerClient::spawn(identity, policy)?;
            let pool =
                VmCapabilityWorkerPool::new(vec![VmCapabilityWorkerPoolSlot::new(client, 32)?])?;
            workers.backends.insert(
                name,
                Backend {
                    pump: VmCapabilityWorkerEventPump::new(pool),
                    deadlines: Vec::new(),
                },
            );
        }
        Ok(workers)
    }

    /// Retains a parked continuation only after closed source-operation and owner checks.
    pub(super) fn submit(
        &mut self,
        name: &str,
        operation: &str,
        arguments: Vec<NativeBoundaryTerm>,
        pending: StorageWait,
    ) -> VmRuntimeResult<()> {
        if pending.deadline <= Instant::now() {
            return Err(recovery_error("deadline expired before submission"));
        }
        let deadline = pending.deadline;
        let backend = self
            .backends
            .get_mut(name)
            .ok_or("error[vm.distributed_storage.binding]: backend is not configured")?;
        let assignment = backend
            .pump
            .submit(
                pending.owner,
                pending.context.clone(),
                operation,
                arguments,
                pending,
            )
            .map_err(|(error, _)| error)?;
        backend.deadlines.push((assignment, deadline));
        Ok(())
    }

    /// Arms transport wakeups before observing completion queues, avoiding lost wakeups.
    pub(super) fn register_waker(&self, waker: &Waker) {
        for backend in self.backends.values() {
            if !backend.deadlines.is_empty() {
                backend.pump.register_event_waker(waker);
            }
        }
    }

    /// Polls one completion; timeout never implies an unacknowledged mutation rolled back.
    pub(super) fn poll(&mut self) -> VmRuntimeResult<Option<StorageCompletion>> {
        if let Some(completed) = self.completed.pop_front() {
            return Ok(Some(completed));
        }
        for backend in self.backends.values_mut() {
            if let Some(index) = backend
                .deadlines
                .iter()
                .position(|(_, deadline)| *deadline <= Instant::now())
            {
                let (assignment, _) = backend.deadlines.swap_remove(index);
                let pending = match backend.pump.cancel(&assignment) {
                    Ok(pending) | Err((_, pending)) => pending,
                };
                return Ok(Some(StorageCompletion {
                    pending,
                    reply: Err(recovery_error("deadline expired")),
                }));
            }
            while let Some(event) = backend.pump.poll()? {
                match event {
                    VmCapabilityWorkerEventPumpEvent::Completed {
                        assignment,
                        context,
                        reply,
                        payload,
                    } => {
                        backend.deadlines.retain(|(live, _)| live != &assignment);
                        let reply = if context == payload.context {
                            Ok(reply)
                        } else {
                            Err(recovery_error("completion authority mismatch"))
                        };
                        return Ok(Some(StorageCompletion {
                            pending: payload,
                            reply,
                        }));
                    }
                    VmCapabilityWorkerEventPumpEvent::WorkerLost { pending, .. } => {
                        backend.deadlines.clear();
                        self.completed
                            .extend(pending.into_iter().map(|(_, pending)| StorageCompletion {
                                pending,
                                reply: Err(recovery_error("storage worker lost")),
                            }));
                        if let Some(completed) = self.completed.pop_front() {
                            return Ok(Some(completed));
                        }
                    }
                    VmCapabilityWorkerEventPumpEvent::Ignored { .. } => {}
                }
            }
        }
        Ok(None)
    }

    /// Returns the next storage deadline; the caller parks only after runnable work is drained.
    pub(super) fn idle_duration(&self) -> Duration {
        self.backends
            .values()
            .flat_map(|backend| backend.deadlines.iter().map(|(_, deadline)| *deadline))
            .min()
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(30))
    }

    /// Revokes queued owner payloads on every exit path; cancellation is not rollback.
    pub(super) fn cancel_owner(&mut self, owner: u64) {
        self.completed
            .retain(|completion| completion.pending.owner.as_u64() != owner);
        for backend in self.backends.values_mut() {
            let assignments: Vec<_> = backend
                .deadlines
                .iter()
                .filter(|(assignment, _)| assignment.owner.as_u64() == owner)
                .map(|(assignment, _)| assignment.clone())
                .collect();
            for assignment in assignments {
                let _cancelled = backend.pump.cancel(&assignment);
                backend.deadlines.retain(|(live, _)| live != &assignment);
            }
        }
    }
}

impl Drop for VmStorageWorkers {
    fn drop(&mut self) {
        for backend in self.backends.values_mut() {
            let _shutdown = backend.pump.shutdown();
        }
    }
}

fn recovery_error(reason: &str) -> VmRuntimeError {
    format!("error[vm.distributed_storage.indeterminate]: {reason}; durable commit may have occurred; inspect and replay the identical checkpoint before further writes").into()
}

#[cfg(test)]
#[path = "storage_transport_test.rs"]
mod tests;
