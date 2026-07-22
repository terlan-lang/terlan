//! Tests for the coarse supervisor-to-shard protocol.

use super::*;

/// Creates one valid control request identity.
fn request(value: u64) -> VmShardControlRequestId {
    VmShardControlRequestId::new(value).expect("control request identity")
}

/// Creates one valid shard identity.
fn shard(value: &str) -> VmExecutionShardId {
    VmExecutionShardId::new(value).expect("execution shard identity")
}

/// Proves the protocol exposes exactly the five coarse control classes.
#[test]
fn supervisor_protocol_exposes_only_coarse_control_classes() {
    let commands = [
        VmShardControlCommand::admission(request(1), "image-v1", [1; 32]).expect("image admission"),
        VmShardControlCommand::lifecycle(request(2), VmShardLifecycleDirective::Drain),
        VmShardControlCommand::inspection(request(3), VmShardInspectionSubject::Health),
        VmShardControlCommand::cross_shard_route(
            request(4),
            shard("shard-a"),
            shard("shard-b"),
            Arc::<[u8]>::from([7, 8, 9]),
        )
        .expect("cross-shard route"),
        VmShardControlCommand::recovery(request(5), shard("shard-c"), 9).expect("shard recovery"),
    ];

    assert_eq!(
        commands
            .iter()
            .map(VmShardControlCommand::class)
            .collect::<Vec<_>>(),
        VmShardControlClass::ALL.to_vec()
    );
    assert_eq!(
        commands
            .iter()
            .map(|command| command.request_id().as_u64())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5]
    );
}

/// Rejects malformed control identities, admission, routing, and recovery.
#[test]
fn supervisor_protocol_rejects_ambiguous_or_unbounded_commands() {
    assert_eq!(
        VmExecutionShardId::new("  "),
        Err(VmShardControlError::EmptyShardId)
    );
    assert_eq!(
        VmShardControlRequestId::new(0),
        Err(VmShardControlError::ZeroRequestId)
    );
    assert_eq!(
        VmShardControlCommand::admission(request(1), "", [1; 32]),
        Err(VmShardControlError::EmptyImageIdentity)
    );
    assert_eq!(
        VmShardControlCommand::admission(request(1), "image-v1", [0; 32]),
        Err(VmShardControlError::EmptyImageDigest)
    );
    assert_eq!(
        VmShardControlCommand::cross_shard_route(
            request(2),
            shard("same"),
            shard("same"),
            Arc::<[u8]>::from([1]),
        ),
        Err(VmShardControlError::SameShardRoute)
    );
    assert_eq!(
        VmShardControlCommand::cross_shard_route(
            request(2),
            shard("source"),
            shard("destination"),
            Arc::<[u8]>::from([]),
        ),
        Err(VmShardControlError::EmptyRouteEnvelope)
    );
    let oversized = Arc::<[u8]>::from(vec![0; MAX_CROSS_SHARD_ENVELOPE_BYTES + 1]);
    assert_eq!(
        VmShardControlCommand::cross_shard_route(
            request(2),
            shard("source"),
            shard("destination"),
            oversized,
        ),
        Err(VmShardControlError::RouteEnvelopeTooLarge {
            actual: MAX_CROSS_SHARD_ENVELOPE_BYTES + 1,
            maximum: MAX_CROSS_SHARD_ENVELOPE_BYTES,
        })
    );
    assert_eq!(
        VmShardControlCommand::recovery(request(3), shard("failed"), 0),
        Err(VmShardControlError::ZeroRecoveryEpoch)
    );
    assert_eq!(
        VmShardEpoch::new(0),
        Err(VmShardControlError::ZeroShardEpoch)
    );
}

/// Exercises every closed lifecycle and inspection value without actor calls.
#[test]
fn coarse_lifecycle_and_inspection_values_are_complete() {
    for directive in [
        VmShardLifecycleDirective::Start,
        VmShardLifecycleDirective::Drain,
        VmShardLifecycleDirective::Stop,
    ] {
        assert_eq!(
            VmShardControlCommand::lifecycle(request(1), directive).class(),
            VmShardControlClass::Lifecycle
        );
    }
    for subject in [
        VmShardInspectionSubject::Health,
        VmShardInspectionSubject::Processes,
        VmShardInspectionSubject::Resources,
    ] {
        assert_eq!(
            VmShardControlCommand::inspection(request(2), subject).class(),
            VmShardControlClass::Inspection
        );
    }
    assert_eq!(shard("shard-a").as_str(), "shard-a");
}
