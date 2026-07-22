use std::sync::Arc;

use super::{
    logical_value_bytes, VmMemoryAccountant, VmMemoryLimits, VmMemoryPressureOutcome,
    VmSharedAllocationKind,
};
use crate::runtime::vm::{
    process::{VmProcessSource, VmProcessTable},
    ReplValue,
};

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.AllocatorParity", function, 0)
}

#[test]
fn alloc_suite_value_replacement_preserves_bytes_and_rejects_pressure_atomically() {
    let mut processes = VmProcessTable::default();
    let sender = processes.spawn_root(source("sender"));
    let owner = processes.spawn_root(source("owner"));
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(128, 224).expect("limits"));
    let original = ReplValue::Bytes(Arc::from(
        (0_u8..96).map(|byte| byte ^ 0x5a).collect::<Vec<_>>(),
    ));
    let replacement = ReplValue::Bytes(Arc::from(
        (0_u8..160)
            .map(|byte| byte.wrapping_mul(17).wrapping_add(3))
            .collect::<Vec<_>>(),
    ));

    let original_bytes = logical_value_bytes(&original).expect("original retained size");
    let first = memory
        .send_value_message(&mut processes, sender, owner, original.clone())
        .expect("initial allocation");
    assert_eq!(first.pressure.requested_bytes, original_bytes);
    assert_eq!(first.pressure.outcome, VmMemoryPressureOutcome::Accounted);
    assert_eq!(
        processes.get(owner).expect("owner").heap_bytes,
        original_bytes
    );
    assert_eq!(
        memory
            .receive_message(&mut processes, owner)
            .expect("receive original")
            .expect("original message")
            .payload,
        original
    );
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);

    let replacement_bytes = logical_value_bytes(&replacement).expect("replacement retained size");
    let replaced = memory
        .send_value_message(&mut processes, sender, owner, replacement.clone())
        .expect("replacement allocation");
    assert_eq!(replaced.pressure.requested_bytes, replacement_bytes);
    assert_eq!(
        replaced.pressure.outcome,
        VmMemoryPressureOutcome::SoftLimitExceeded
    );
    let received = memory
        .receive_message(&mut processes, owner)
        .expect("receive replacement")
        .expect("replacement message");
    assert_eq!(received.payload, replacement);
    assert_eq!(received.accounted_bytes, replacement_bytes);
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);

    let oversized = ReplValue::Bytes(Arc::from(vec![0xa5; 224]));
    let rejected = memory
        .send_value_message(&mut processes, sender, owner, oversized)
        .expect("pressure decision");
    assert_eq!(
        rejected.pressure.outcome,
        VmMemoryPressureOutcome::HardLimitRejected
    );
    assert_eq!(rejected.published_message_id(), None);
    assert_eq!(processes.get(owner).expect("owner").mailbox_len(), 0);
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);

    let metrics = memory.process_metrics(owner).expect("owner metrics");
    assert_eq!(metrics.current_bytes, 0);
    assert_eq!(metrics.high_water_bytes, replacement_bytes);
    assert_eq!(metrics.released_bytes, original_bytes + replacement_bytes);
}

#[test]
fn alloc_suite_shared_lifetime_and_owner_isolation_are_leak_free() {
    let mut processes = VmProcessTable::default();
    let owners = (0..4)
        .map(|index| processes.spawn_root(source(&format!("owner_{index}"))))
        .collect::<Vec<_>>();
    let pressured = processes.spawn_root(source("pressured"));
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(96, 128).expect("limits"));

    for round in 0..32 {
        let owner = owners[round % owners.len()];
        let bytes = 24 + (round % 3) * 8;
        let decision = memory
            .account_heap(&mut processes, owner, bytes)
            .expect("owner allocation");
        assert_eq!(decision.outcome, VmMemoryPressureOutcome::Accounted);
        assert_eq!(processes.get(owner).expect("owner").heap_bytes, bytes);
        assert_eq!(
            memory
                .release_heap(&mut processes, owner, bytes)
                .expect("owner release"),
            bytes
        );
        assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    }
    assert!(owners.iter().all(|owner| memory
        .process_metrics(*owner)
        .expect("metrics")
        .current_bytes
        == 0));

    let registered = memory
        .register_shared_allocation(
            &mut processes,
            owners[0],
            VmSharedAllocationKind::NativeBoundaryBuffer,
            48,
        )
        .expect("shared allocation");
    let allocation = registered.allocation_id.expect("allocation id");
    for owner in &owners[1..] {
        let retained = memory
            .retain_shared_allocation(&mut processes, allocation, owners[0], *owner)
            .expect("retain shared allocation");
        assert_eq!(retained.allocation_id, Some(allocation));
        assert_eq!(processes.get(*owner).expect("owner").heap_bytes, 48);
    }

    memory
        .account_heap(&mut processes, pressured, 96)
        .expect("pressure preload");
    let rejected = memory
        .retain_shared_allocation(&mut processes, allocation, owners[0], pressured)
        .expect("typed pressure rejection");
    assert_eq!(rejected.allocation_id, None);
    assert_eq!(
        rejected.pressure.outcome,
        VmMemoryPressureOutcome::HardLimitRejected
    );
    assert_eq!(processes.get(pressured).expect("pressured").heap_bytes, 96);

    memory
        .reclassify_shared_allocation(
            allocation,
            owners[0],
            VmSharedAllocationKind::NativeBoundaryBuffer,
            VmSharedAllocationKind::ResponseBuffer,
        )
        .expect("logical replacement");
    assert_eq!(
        memory.shared_allocation_kind(allocation),
        Some(VmSharedAllocationKind::ResponseBuffer)
    );

    for (index, owner) in owners.iter().enumerate() {
        let deallocated = memory
            .release_shared_allocation(&mut processes, allocation, *owner)
            .expect("release owner reference");
        assert_eq!(deallocated, index + 1 == owners.len());
        assert_eq!(processes.get(*owner).expect("owner").heap_bytes, 0);
    }
    assert_eq!(memory.shared_allocation_kind(allocation), None);
    assert_eq!(
        memory
            .release_heap(&mut processes, pressured, 96)
            .expect("release pressure preload"),
        96
    );
    assert_eq!(processes.get(pressured).expect("pressured").heap_bytes, 0);
    assert!(owners.iter().chain([&pressured]).all(|owner| memory
        .process_metrics(*owner)
        .expect("final metrics")
        .current_bytes
        == 0));
}
