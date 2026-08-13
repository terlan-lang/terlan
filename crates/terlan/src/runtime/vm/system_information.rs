#[cfg(test)]
use super::process_environment::VmRuntimeEnvironmentSnapshot;

/// Portable, immutable identity and capacity information for one VM runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSystemInformationSnapshot {
    pub(crate) schema: &'static str,
    pub(crate) runtime_name: &'static str,
    pub(crate) runtime_version: &'static str,
    pub(crate) target_architecture: &'static str,
    pub(crate) process_limit: usize,
    pub(crate) scheduler_count: usize,
    pub(crate) word_size_bytes: usize,
    pub(crate) process_count: usize,
    pub(crate) exited_process_count: usize,
    pub(crate) run_queue_length: usize,
    pub(crate) mailbox_message_count: usize,
    pub(crate) logical_heap_bytes: usize,
    pub(crate) resource_handle_count: usize,
    pub(crate) active_timer_count: usize,
}

#[cfg(test)]
impl VmSystemInformationSnapshot {
    /// Projects an owned runtime environment snapshot onto the stable system
    /// information surface. Exited process records remain separately visible
    /// and never inflate the live process count.
    pub(crate) fn from_environment(environment: &VmRuntimeEnvironmentSnapshot) -> Self {
        Self {
            schema: "terlan-vm-system-information-v1",
            runtime_name: "terlan-vm",
            runtime_version: env!("CARGO_PKG_VERSION"),
            target_architecture: std::env::consts::ARCH,
            process_limit: environment.process_limit,
            scheduler_count: environment.scheduler_count,
            word_size_bytes: environment.word_size_bytes,
            process_count: environment.live_processes,
            exited_process_count: environment.exited_processes,
            run_queue_length: environment.run_queue,
            mailbox_message_count: environment.mailbox_messages,
            logical_heap_bytes: environment.logical_heap_bytes,
            resource_handle_count: environment.resource_handles,
            active_timer_count: environment.active_timers,
        }
    }
}
