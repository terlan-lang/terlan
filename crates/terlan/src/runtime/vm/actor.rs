#![allow(dead_code)]

include!("actor_impl.rs");

pub(crate) use self::actor_suspension::VmNativeTimerWait;
pub(crate) use self::actor_timer::VmActorTimerAdvance;
#[cfg(test)]
pub(crate) use self::actor_timer::VmActorTimerDelivery;

#[path = "actor/transfer.rs"]
mod transfer;

pub(crate) use transfer::{VmActorRuntimeImportFailure, VmActorRuntimeTransfer};

vm_capability_component! {
    #[path = "actor_capability.rs"]
    mod actor_capability;
}

#[cfg(test)]
#[path = "actor_test.rs"]
mod actor_test;

#[cfg(test)]
#[path = "actor_exit_accounting_test.rs"]
mod actor_exit_accounting_test;

#[cfg(test)]
#[path = "actor_checkpoint_accounting_test.rs"]
mod actor_checkpoint_accounting_test;

#[cfg(test)]
#[path = "actor_suspension_test.rs"]
mod actor_suspension_test;

#[cfg(test)]
#[path = "actor_native_failure_test.rs"]
mod actor_native_failure_test;

#[cfg(test)]
#[path = "actor_native_scheduling_test.rs"]
mod actor_native_scheduling_test;

#[cfg(test)]
#[path = "actor_dirty_bif_beam_suite_parity_test.rs"]
mod actor_dirty_bif_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_dirty_nif_beam_suite_parity_test.rs"]
mod actor_dirty_nif_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_suspension_accounting_test.rs"]
mod actor_suspension_accounting_test;

#[cfg(test)]
#[path = "actor_alias_test.rs"]
mod actor_alias_test;

#[cfg(test)]
#[path = "actor_identity_test.rs"]
mod actor_identity_test;

#[cfg(test)]
#[path = "actor_runtime_transfer_test.rs"]
mod actor_runtime_transfer_test;

#[cfg(test)]
#[path = "actor_send_test.rs"]
mod actor_send_test;

#[cfg(test)]
#[path = "actor_send_accounting_test.rs"]
mod actor_send_accounting_test;

#[cfg(test)]
#[path = "actor_timer_test.rs"]
mod actor_timer_test;

#[cfg(test)]
#[path = "actor_timer_accounting_test.rs"]
mod actor_timer_accounting_test;

#[cfg(test)]
#[path = "actor_timer_cancellation_accounting_test.rs"]
mod actor_timer_cancellation_accounting_test;

#[cfg(test)]
#[path = "actor_timer_options_test.rs"]
mod actor_timer_options_test;

#[cfg(test)]
#[path = "actor_timer_parity_test.rs"]
mod actor_timer_parity_test;

#[cfg(test)]
#[path = "actor_timer_bif_beam_suite_parity_test.rs"]
mod actor_timer_bif_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_spawn_test.rs"]
mod actor_spawn_test;

#[cfg(test)]
#[path = "actor_relationship_test.rs"]
mod actor_relationship_test;

#[cfg(test)]
#[path = "actor_relationship_accounting_test.rs"]
mod actor_relationship_accounting_test;

#[cfg(test)]
#[path = "actor_monitor_beam_suite_parity_test.rs"]
mod actor_monitor_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_registry_accounting_test.rs"]
mod actor_registry_accounting_test;

#[cfg(test)]
#[path = "actor_signal_test.rs"]
mod actor_signal_test;

#[cfg(test)]
#[path = "actor_signal_beam_suite_parity_test.rs"]
mod actor_signal_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_process_beam_suite_parity_test.rs"]
mod actor_process_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_process_max_heap_size_beam_suite_parity_test.rs"]
mod actor_process_max_heap_size_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_lifecycle_test.rs"]
mod actor_lifecycle_test;

#[cfg(test)]
#[path = "actor_observability_test.rs"]
mod actor_observability_test;

#[cfg(test)]
#[path = "actor_statistics_beam_suite_parity_test.rs"]
mod actor_statistics_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_system_info_beam_suite_parity_test.rs"]
mod actor_system_info_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_system_profile_beam_suite_parity_test.rs"]
mod actor_system_profile_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_z_beam_suite_parity_test.rs"]
mod actor_z_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_trace_beam_suite_parity_test.rs"]
mod actor_trace_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_call_trace_beam_suite_parity_test.rs"]
mod actor_call_trace_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_trace_bif_beam_suite_parity_test.rs"]
mod actor_trace_bif_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_trace_call_count_beam_suite_parity_test.rs"]
mod actor_trace_call_count_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_trace_call_memory_beam_suite_parity_test.rs"]
mod actor_trace_call_memory_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_trace_call_time_beam_suite_parity_test.rs"]
mod actor_trace_call_time_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_trace_local_beam_suite_parity_test.rs"]
mod actor_trace_local_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_trace_meta_beam_suite_parity_test.rs"]
mod actor_trace_meta_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_trace_nif_beam_suite_parity_test.rs"]
mod actor_trace_nif_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_trace_port_beam_suite_parity_test.rs"]
mod actor_trace_port_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_trace_session_beam_suite_parity_test.rs"]
mod actor_trace_session_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_tracer_beam_suite_parity_test.rs"]
mod actor_tracer_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_receive_accounting_test.rs"]
mod actor_receive_accounting_test;

#[cfg(test)]
#[path = "actor_receive_beam_suite_parity_test.rs"]
mod actor_receive_beam_suite_parity_test;

#[cfg(test)]
#[path = "actor_shard_service_isolation_test.rs"]
mod actor_shard_service_isolation_test;
