//! Local native-image admission and shard identity helpers.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::vm::execution_shard_protocol::{
    VmExecutionShardId, VmSealedShardImage, VmShardEpoch,
};
use crate::runtime::vm::execution_shard_supervisor::{
    VmExecutionShardSupervisor, VmShardProtocolVersion, VmShardSupervisorPolicy,
};
use crate::runtime::vm::process::VmProcessSource;
use crate::runtime::vm::supervision::VmRestartBackoffSchedule;

use super::super::{
    PureNativeBoundary, PureNativeExecutionRuntime, VmNativeGenerationReferenceSnapshot,
};

const LOCAL_SHARD_PROTOCOL_VERSION: u16 = 1;
const LOCAL_SHARD_RESTART_BUDGET: u32 = 3;
const LOCAL_SHARD_RESTART_INITIAL_TICKS: u64 = 10;
const LOCAL_SHARD_RESTART_MAX_TICKS: u64 = 1_000;

pub(super) fn load_image_components(
    path: &Path,
) -> Result<(PureNativeBoundary, PureNativeExecutionRuntime), String> {
    let (boundary, managed) = PureNativeBoundary::load_image(path)?;
    Ok((boundary, PureNativeExecutionRuntime::from_managed(managed)))
}

pub(super) fn admit_supervisor(
    shard_id: VmExecutionShardId,
    image: VmSealedShardImage,
) -> Result<VmExecutionShardSupervisor, String> {
    let protocol = local_protocol_version();
    let policy = VmShardSupervisorPolicy::new(
        protocol,
        LOCAL_SHARD_RESTART_BUDGET,
        VmRestartBackoffSchedule::exponential(
            LOCAL_SHARD_RESTART_INITIAL_TICKS,
            LOCAL_SHARD_RESTART_MAX_TICKS,
        ),
    );
    let mut supervisor = VmExecutionShardSupervisor::new(shard_id, policy);
    supervisor
        .begin_negotiation()
        .map_err(|error| lifecycle_error("begin native shard negotiation", error))?;
    supervisor
        .negotiate(protocol)
        .map_err(|error| lifecycle_error("negotiate native shard", error))?;
    let epoch = supervisor
        .admit_image(image)
        .map_err(|error| lifecycle_error("admit native shard image", error))?;
    supervisor
        .acknowledge_ready(epoch)
        .map_err(|error| lifecycle_error("publish native shard readiness", error))?;
    supervisor
        .signal_health(epoch, 1)
        .map_err(|error| lifecycle_error("publish native shard health", error))?;
    Ok(supervisor)
}

pub(super) fn local_protocol_version() -> VmShardProtocolVersion {
    VmShardProtocolVersion::new(LOCAL_SHARD_PROTOCOL_VERSION)
        .expect("constant local shard protocol version must be nonzero")
}

pub(super) fn allocate_sequence(sequence: &AtomicU64, kind: &str) -> Result<u64, String> {
    sequence
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| format!("error[execution_shard.identity]: {kind} identity space exhausted"))
}

pub(super) fn shard_identity(
    image_identity: &str,
    sequence: u64,
) -> Result<VmExecutionShardId, String> {
    VmExecutionShardId::new(format!("{image_identity}.shard-{sequence}"))
        .map_err(|error| lifecycle_error("create native shard identity", error))
}

pub(super) fn lifecycle_error(context: &str, error: impl std::fmt::Debug) -> String {
    format!("error[execution_shard.lifecycle]: {context}: {error:?}")
}

pub(super) fn pending_generation_error(
    epoch: VmShardEpoch,
    references: &VmNativeGenerationReferenceSnapshot,
) -> String {
    format!(
        "error[execution_shard.generation_references]: epoch={} total={} references={}",
        epoch.as_u64(),
        references.total(),
        references.render_pending()
    )
}

pub(super) fn call_source(function: &str, arity: usize) -> VmProcessSource {
    let (module, function) = function
        .rsplit_once('.')
        .unwrap_or(("native.Image", function));
    VmProcessSource::new(module, function, arity)
}
