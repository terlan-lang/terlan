//! Shared construction helpers for execution-shard lifecycle tests.

use super::{
    VmExecutionShardSupervisor, VmShardPhase, VmShardProtocolVersion, VmShardSupervisorPolicy,
};
use crate::runtime::vm::execution_shard_protocol::{
    VmExecutionShardId, VmSealedShardImage, VmShardEpoch,
};
use crate::runtime::vm::restart_backoff::VmRestartBackoffSchedule;

/// Creates the protocol version used by lifecycle tests.
pub(super) fn protocol(value: u16) -> VmShardProtocolVersion {
    VmShardProtocolVersion::new(value).expect("protocol version")
}

/// Creates a validated shard identity.
pub(super) fn shard_id() -> VmExecutionShardId {
    VmExecutionShardId::new("primary").expect("shard identity")
}

/// Creates sealed image metadata with a distinct digest byte.
pub(super) fn image(name: &str, digest: u8) -> VmSealedShardImage {
    VmSealedShardImage::new(name, [digest; 32]).expect("sealed image")
}

/// Creates a supervisor with the requested restart budget.
pub(super) fn supervisor_with_budget(restart_budget: u32) -> VmExecutionShardSupervisor {
    VmExecutionShardSupervisor::new(
        shard_id(),
        VmShardSupervisorPolicy::new(
            protocol(1),
            restart_budget,
            VmRestartBackoffSchedule::exponential(10, 20),
        ),
    )
}

/// Creates a supervisor with the default two-restart test policy.
pub(super) fn supervisor() -> VmExecutionShardSupervisor {
    supervisor_with_budget(2)
}

/// Negotiates, admits, and acknowledges one ready image generation.
pub(super) fn make_ready(
    supervisor: &mut VmExecutionShardSupervisor,
    name: &str,
    digest: u8,
) -> VmShardEpoch {
    if supervisor.phase() == VmShardPhase::Created {
        supervisor.begin_negotiation().expect("begin negotiation");
    }
    supervisor.negotiate(protocol(1)).expect("negotiate");
    let epoch = supervisor
        .admit_image(image(name, digest))
        .expect("admit image");
    assert!(!supervisor.is_routable());
    supervisor
        .acknowledge_ready(epoch)
        .expect("acknowledge ready");
    epoch
}

/// Restarts one crashed shard exactly at its owned backoff deadline.
pub(super) fn restart_when_due(supervisor: &mut VmExecutionShardSupervisor) {
    let deadline = supervisor
        .restart_deadline_tick()
        .expect("crashed shard restart deadline");
    supervisor
        .restart_when_due(deadline)
        .expect("restart at deadline");
}
