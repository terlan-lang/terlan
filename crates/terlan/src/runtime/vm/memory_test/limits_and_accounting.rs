use std::path::PathBuf;

use super::super::super::{
    bitstring::VmBitString,
    map_value::VmMapValue,
    process::{VmExitReason, VmProcessSource, VmProcessState, VmProcessTable},
    resource::{VmResourceDescriptor, VmResourceEvent, VmResourceTable, VmResourceTransferPolicy},
    ReplValue,
};
use super::super::{
    logical_value_bytes, VmMemoryAccountant, VmMemoryLimits, VmMemoryPressureOutcome,
    VmSharedAllocationKind, VmValueSizeError,
};

pub(super) fn source() -> VmProcessSource {
    VmProcessSource::new("app.Main", "memory_worker", 0)
}

#[test]
pub(super) fn memory_limits_reject_invalid_thresholds() {
    assert_eq!(
        VmMemoryLimits::new(0, 1).expect_err("zero soft limit"),
        "VM memory soft limit must be greater than zero"
    );
    assert_eq!(
        VmMemoryLimits::new(10, 9).expect_err("hard below soft"),
        "VM memory hard limit must be greater than or equal to soft limit"
    );
}

#[test]
pub(super) fn memory_accounting_enforces_soft_and_hard_limits_without_partial_mutation() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source());
    let limits = VmMemoryLimits::new(100, 150).expect("limits");
    let mut memory = VmMemoryAccountant::new(limits);

    assert_eq!(
        memory
            .account_heap(&mut processes, pid, 80)
            .expect("account below soft")
            .outcome,
        VmMemoryPressureOutcome::Accounted
    );
    assert_eq!(
        memory
            .account_heap(&mut processes, pid, 30)
            .expect("account above soft")
            .outcome,
        VmMemoryPressureOutcome::SoftLimitExceeded
    );
    let rejected = memory
        .account_heap(&mut processes, pid, 41)
        .expect("hard pressure is typed, not a host error");
    assert_eq!(rejected.outcome, VmMemoryPressureOutcome::HardLimitRejected);
    assert_eq!(rejected.projected_bytes, 151);
    assert_eq!(processes.get(pid).expect("process").heap_bytes, 110);
    let metrics = memory.process_metrics(pid).expect("memory metrics");
    assert_eq!(metrics.current_bytes, 110);
    assert_eq!(metrics.high_water_bytes, 110);

    let overflow_pid = processes.spawn_root(source());
    let overflow_limits = VmMemoryLimits::new(1, usize::MAX).expect("overflow limits");
    let mut overflow_memory = VmMemoryAccountant::new(overflow_limits);
    overflow_memory
        .account_heap(&mut processes, overflow_pid, 1)
        .expect("initial allocation");
    let overflow = overflow_memory
        .account_heap(&mut processes, overflow_pid, usize::MAX)
        .expect("overflow is a typed pressure decision");
    assert_eq!(overflow.outcome, VmMemoryPressureOutcome::HardLimitRejected);
    assert_eq!(overflow.projected_bytes, usize::MAX);
    assert_eq!(processes.get(overflow_pid).expect("process").heap_bytes, 1);
}

#[test]
pub(super) fn memory_release_is_saturating_and_process_exit_clears_heap_state() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source());
    let limits = VmMemoryLimits::new(100, 200).expect("limits");
    let mut memory = VmMemoryAccountant::new(limits);
    memory
        .account_heap(&mut processes, pid, 90)
        .expect("account heap");

    assert_eq!(memory.release_heap(&mut processes, pid, 30).unwrap(), 30);
    assert_eq!(memory.release_heap(&mut processes, pid, 100).unwrap(), 60);
    let metrics = memory.process_metrics(pid).expect("memory metrics");
    assert_eq!(metrics.current_bytes, 0);
    assert_eq!(metrics.high_water_bytes, 90);
    assert_eq!(metrics.collection_events, 2);
    assert_eq!(metrics.released_bytes, 90);

    memory
        .account_heap(&mut processes, pid, 40)
        .expect("account before exit");
    processes
        .exit_process(pid, VmExitReason::Normal)
        .expect("process exit");
    memory
        .synchronize_process(&processes, pid)
        .expect("synchronize exited process");
    assert_eq!(processes.get(pid).expect("process").heap_bytes, 0);
    let metrics = memory.process_metrics(pid).expect("memory metrics");
    assert_eq!(metrics.current_bytes, 0);
    assert_eq!(metrics.high_water_bytes, 90);
    assert_eq!(metrics.collection_events, 3);
    assert_eq!(metrics.released_bytes, 130);
}

#[test]
pub(super) fn memory_accounting_rejects_missing_and_exited_processes() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source());
    processes
        .exit_process(pid, VmExitReason::Normal)
        .expect("process exit");
    let limits = VmMemoryLimits::new(100, 200).expect("limits");
    let mut memory = VmMemoryAccountant::new(limits);

    assert_eq!(
        memory
            .account_heap(&mut processes, pid, 1)
            .expect_err("exited process"),
        "exited process 1 cannot own VM heap bytes"
    );
    let missing = crate::runtime::vm::process::VmProcessId::from_raw_for_test(99);
    assert_eq!(
        memory
            .release_heap(&mut processes, missing, 1)
            .expect_err("missing process"),
        "missing process 99 for VM memory accounting"
    );
}

#[test]
pub(super) fn memory_accounting_writes_deterministic_pressure_report() {
    let mut processes = VmProcessTable::default();
    let retained = processes.spawn_root(source());
    let released = processes.spawn_root(source());
    let limits = VmMemoryLimits::new(64, 128).expect("limits");
    let mut memory = VmMemoryAccountant::new(limits);
    let mut resources = VmResourceTable::default();
    memory
        .account_heap(&mut processes, retained, 65)
        .expect("soft pressure");
    memory
        .register_resource(
            &mut processes,
            &mut resources,
            retained,
            VmResourceDescriptor::new("file.handle", "/tmp/report"),
            VmResourceTransferPolicy::OwnerOnly,
            16,
        )
        .expect("accounted resource");
    memory
        .register_shared_allocation(&mut processes, retained, VmSharedAllocationKind::Binary, 24)
        .expect("shared binary");
    memory
        .account_heap(&mut processes, released, 32)
        .expect("released allocation");
    memory
        .release_heap(&mut processes, released, 32)
        .expect("release allocation");
    let report_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/quality/vm-memory-pressure-report.json");

    memory
        .write_pressure_report(&report_path)
        .expect("write memory pressure report");
    let report = std::fs::read_to_string(report_path).expect("read report");
    let report: serde_json::Value = serde_json::from_str(&report).expect("parse report");
    assert_eq!(report["schema"], "terlan-vm-memory-pressure-report-v1");
    assert_eq!(report["limits"]["softBytes"], 64);
    assert_eq!(report["limits"]["hardBytes"], 128);
    assert_eq!(report["processMetrics"].as_array().unwrap().len(), 2);
    assert_eq!(report["pressureDecisions"].as_array().unwrap().len(), 4);
    assert_eq!(report["resourceOwnership"].as_array().unwrap().len(), 1);
    assert_eq!(report["resourceOwnership"][0]["owner"], 1);
    assert_eq!(report["resourceOwnership"][0]["logicalBytes"], 16);
    assert_eq!(report["sharedAllocationCounts"]["activeAllocations"], 1);
    assert_eq!(report["sharedAllocationCounts"]["uniqueLogicalBytes"], 24);
    assert_eq!(report["sharedAllocationCounts"]["ownerReferences"], 1);
    assert_eq!(report["sharedAllocations"][0]["kind"], "binary");
    assert_eq!(report["sharedAllocations"][0]["owners"][0], 1);
    assert_eq!(
        report["leakClassifications"][0]["classification"],
        "retained_live"
    );
    assert_eq!(
        report["leakClassifications"][1]["classification"],
        "released"
    );
}

#[test]
pub(super) fn memory_accounted_mailbox_send_receive_and_pressure_are_atomic() {
    let mut processes = VmProcessTable::default();
    let sender = processes.spawn_root(source());
    let recipient = processes.spawn_root(source());
    let limits = VmMemoryLimits::new(50, 80).expect("limits");
    let mut memory = VmMemoryAccountant::new(limits);

    let first = memory
        .send_message(
            &mut processes,
            sender,
            recipient,
            ReplValue::Atom("first".to_string()),
            40,
        )
        .expect("first message");
    assert_eq!(first.published_message_id(), Some(1));
    let publication = first.publication.expect("accepted send is published");
    assert_eq!(publication.recipient(), recipient);
    assert_eq!(publication.accounted_bytes(), 40);
    assert_eq!(
        processes.get(recipient).expect("recipient").mailbox_len(),
        1
    );
    assert_eq!(first.pressure.outcome, VmMemoryPressureOutcome::Accounted);
    let second = memory
        .send_message(
            &mut processes,
            sender,
            recipient,
            ReplValue::Atom("second".to_string()),
            30,
        )
        .expect("soft-pressure message");
    assert_eq!(second.published_message_id(), Some(2));
    assert_eq!(
        second.pressure.outcome,
        VmMemoryPressureOutcome::SoftLimitExceeded
    );
    let rejected = memory
        .send_message(
            &mut processes,
            sender,
            recipient,
            ReplValue::Atom("rejected".to_string()),
            20,
        )
        .expect("hard pressure is typed");
    assert_eq!(rejected.published_message_id(), None);
    assert_eq!(
        rejected.pressure.outcome,
        VmMemoryPressureOutcome::HardLimitRejected
    );
    assert_eq!(
        processes.get(recipient).expect("recipient").mailbox_len(),
        2
    );
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 70);

    let first = memory
        .receive_message(&mut processes, recipient)
        .expect("receive first")
        .expect("first message");
    assert_eq!(first.id, 1);
    assert_eq!(first.accounted_bytes, 40);
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 30);
    let second = memory
        .receive_message(&mut processes, recipient)
        .expect("receive second")
        .expect("second message");
    assert_eq!(second.id, 2);
    assert_eq!(second.accounted_bytes, 30);
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 0);
}

#[test]
pub(super) fn memory_mailbox_routes_validate_before_heap_mutation_and_exit_releases_queue() {
    let mut processes = VmProcessTable::default();
    let sender = processes.spawn_root(source());
    let recipient = processes.spawn_root(source());
    let limits = VmMemoryLimits::new(64, 128).expect("limits");
    let mut memory = VmMemoryAccountant::new(limits);
    let missing = crate::runtime::vm::process::VmProcessId::from_raw_for_test(99);

    assert_eq!(
        memory
            .send_message(&mut processes, missing, recipient, ReplValue::Unit, 32,)
            .expect_err("missing sender"),
        "missing sender process 99"
    );
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 0);
    memory
        .send_message(&mut processes, sender, recipient, ReplValue::Unit, 32)
        .expect("accounted message");
    processes
        .exit_process(recipient, VmExitReason::Killed)
        .expect("recipient exit");
    memory
        .synchronize_process(&processes, recipient)
        .expect("synchronize recipient exit");

    let recipient = processes.get(recipient).expect("recipient");
    assert_eq!(recipient.heap_bytes, 0);
    assert_eq!(recipient.mailbox_len(), 0);
    let metrics = memory
        .process_metrics(recipient.pid)
        .expect("memory metrics");
    assert_eq!(metrics.current_bytes, 0);
    assert_eq!(metrics.high_water_bytes, 32);
    assert_eq!(metrics.released_bytes, 32);
}

#[test]
pub(super) fn memory_selective_receive_releases_selected_charge_and_preserves_skipped_entries() {
    let mut processes = VmProcessTable::default();
    let sender = processes.spawn_root(source());
    let recipient = processes.spawn_root(source());
    let limits = VmMemoryLimits::new(100, 200).expect("limits");
    let mut memory = VmMemoryAccountant::new(limits);
    for (label, bytes) in [("first", 10), ("target", 20), ("last", 30)] {
        memory
            .send_message(
                &mut processes,
                sender,
                recipient,
                ReplValue::Atom(label.to_string()),
                bytes,
            )
            .expect("accounted message");
    }

    let selected = memory
        .selective_receive_message(&mut processes, recipient, |message| {
            message.payload == ReplValue::Atom("target".to_string())
        })
        .expect("selective receive")
        .expect("target message");
    assert_eq!(selected.id, 2);
    assert_eq!(selected.accounted_bytes, 20);
    assert_eq!(
        processes.get(recipient).expect("recipient").mailbox_len(),
        2
    );
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 40);

    let no_match = memory
        .selective_receive_message(&mut processes, recipient, |message| {
            message.payload == ReplValue::Atom("missing".to_string())
        })
        .expect("no-match receive");
    assert_eq!(no_match, None);
    assert_eq!(
        processes.get(recipient).expect("recipient").mailbox_len(),
        2
    );
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 40);

    assert_eq!(
        memory
            .receive_message(&mut processes, recipient)
            .expect("first receive")
            .expect("first message")
            .id,
        1
    );
    assert_eq!(
        memory
            .receive_message(&mut processes, recipient)
            .expect("last receive")
            .expect("last message")
            .id,
        3
    );
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 0);
}

#[test]
pub(super) fn memory_resource_registration_rejects_hard_pressure_without_handle_mutation() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source());
    let mut resources = VmResourceTable::default();
    let limits = VmMemoryLimits::new(40, 50).expect("limits");
    let mut memory = VmMemoryAccountant::new(limits);
    memory
        .account_heap(&mut processes, owner, 30)
        .expect("existing heap");

    let rejected = memory
        .register_resource(
            &mut processes,
            &mut resources,
            owner,
            VmResourceDescriptor::new("file.handle", "/tmp/rejected"),
            VmResourceTransferPolicy::Transferable,
            21,
        )
        .expect("hard pressure is typed");

    assert_eq!(rejected.event, None);
    assert_eq!(
        rejected.pressure.outcome,
        VmMemoryPressureOutcome::HardLimitRejected
    );
    assert!(resources.snapshots().is_empty());
    assert!(processes
        .get(owner)
        .expect("owner")
        .resource_handles
        .is_empty());
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 30);
}

#[test]
pub(super) fn memory_resource_transfer_and_release_move_logical_ownership_atomically() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source());
    let recipient = processes.spawn_root(source());
    let mut resources = VmResourceTable::default();
    let limits = VmMemoryLimits::new(50, 60).expect("limits");
    let mut memory = VmMemoryAccountant::new(limits);
    let registered = memory
        .register_resource(
            &mut processes,
            &mut resources,
            owner,
            VmResourceDescriptor::new("postgres.connection", "primary"),
            VmResourceTransferPolicy::Transferable,
            40,
        )
        .expect("resource registration");
    let Some(VmResourceEvent::Registered { id, .. }) = registered.event else {
        panic!("expected registered resource event");
    };
    memory
        .account_heap(&mut processes, recipient, 30)
        .expect("recipient heap");

    let rejected = memory
        .transfer_resource(&mut processes, &mut resources, id, owner, recipient)
        .expect("hard transfer pressure is typed");
    assert_eq!(rejected.event, None);
    assert_eq!(
        rejected.pressure.outcome,
        VmMemoryPressureOutcome::HardLimitRejected
    );
    assert_eq!(
        resources.get_for_owner(owner, id).expect("owner").owner,
        owner
    );
    assert_eq!(
        memory.resource_ownership(id).expect("memory owner").owner,
        1
    );
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 40);
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 30);

    memory
        .release_heap(&mut processes, recipient, 30)
        .expect("release recipient heap");
    let transferred = memory
        .transfer_resource(&mut processes, &mut resources, id, owner, recipient)
        .expect("resource transfer");
    assert_eq!(
        transferred.event,
        Some(VmResourceEvent::Transferred {
            id,
            from: owner,
            to: recipient,
        })
    );
    assert_eq!(
        memory.resource_ownership(id).expect("memory owner").owner,
        2
    );
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 40);

    assert_eq!(
        memory
            .release_resource(&mut processes, &mut resources, recipient, id)
            .expect("resource release"),
        VmResourceEvent::Released {
            id,
            owner: recipient,
        }
    );
    assert!(memory.resource_ownership(id).is_none());
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 0);
    assert!(processes
        .get(recipient)
        .expect("recipient")
        .resource_handles
        .is_empty());
}

#[test]
pub(super) fn memory_process_exit_releases_resources_mailbox_and_heap_without_cross_owner_cleanup()
{
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source());
    let survivor = processes.spawn_root(source());
    let mut resources = VmResourceTable::default();
    let limits = VmMemoryLimits::new(100, 200).expect("limits");
    let mut memory = VmMemoryAccountant::new(limits);
    memory
        .account_heap(&mut processes, owner, 5)
        .expect("ordinary owner heap");
    memory
        .send_message(&mut processes, owner, owner, ReplValue::Unit, 10)
        .expect("owner mailbox");
    let first = memory
        .register_resource(
            &mut processes,
            &mut resources,
            owner,
            VmResourceDescriptor::new("file.handle", "/tmp/first"),
            VmResourceTransferPolicy::OwnerOnly,
            20,
        )
        .expect("first owner resource");
    let second = memory
        .register_resource(
            &mut processes,
            &mut resources,
            owner,
            VmResourceDescriptor::new("file.handle", "/tmp/second"),
            VmResourceTransferPolicy::OwnerOnly,
            30,
        )
        .expect("second owner resource");
    let survivor_resource = memory
        .register_resource(
            &mut processes,
            &mut resources,
            survivor,
            VmResourceDescriptor::new("file.handle", "/tmp/survivor"),
            VmResourceTransferPolicy::OwnerOnly,
            7,
        )
        .expect("survivor resource");
    let Some(VmResourceEvent::Registered { id: first_id, .. }) = first.event else {
        panic!("expected first registration");
    };
    let Some(VmResourceEvent::Registered { id: second_id, .. }) = second.event else {
        panic!("expected second registration");
    };
    let Some(VmResourceEvent::Registered {
        id: survivor_id, ..
    }) = survivor_resource.event
    else {
        panic!("expected survivor registration");
    };

    let exit = memory
        .exit_process_with_memory_cleanup(
            &mut processes,
            &mut resources,
            owner,
            VmExitReason::Killed,
        )
        .expect("accounted process exit");

    assert_eq!(
        exit.resource_events,
        vec![
            VmResourceEvent::CleanedUpOnExit {
                id: first_id,
                owner,
            },
            VmResourceEvent::CleanedUpOnExit {
                id: second_id,
                owner,
            },
        ]
    );
    assert!(exit.remaining_cleanup_handles.is_empty());
    assert!(exit.released_shared_allocations.is_empty());
    let owner_process = processes.get(owner).expect("owner process");
    assert_eq!(
        owner_process.state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
    assert_eq!(owner_process.heap_bytes, 0);
    assert_eq!(owner_process.mailbox_len(), 0);
    assert!(owner_process.resource_handles.is_empty());
    assert!(memory.resource_ownership(first_id).is_none());
    assert!(memory.resource_ownership(second_id).is_none());
    assert_eq!(
        memory
            .resource_ownership(survivor_id)
            .expect("survivor memory owner")
            .logical_bytes,
        7
    );
    assert_eq!(resources.snapshots().len(), 1);
    assert_eq!(resources.snapshots()[0].id, survivor_id);
    let metrics = memory.process_metrics(owner).expect("owner memory metrics");
    assert_eq!(metrics.current_bytes, 0);
    assert_eq!(metrics.high_water_bytes, 65);
    assert_eq!(metrics.released_bytes, 65);
}

#[test]
pub(super) fn memory_process_exit_rejects_unaccounted_resource_without_mutation() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source());
    let mut resources = VmResourceTable::default();
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(100, 200).expect("limits"));
    let VmResourceEvent::Registered { id, .. } = resources
        .register(
            &mut processes,
            owner,
            VmResourceDescriptor::new("file.handle", "/tmp/unaccounted"),
            VmResourceTransferPolicy::OwnerOnly,
        )
        .expect("unaccounted resource")
    else {
        panic!("expected registration");
    };

    assert_eq!(
        memory
            .exit_process_with_memory_cleanup(
                &mut processes,
                &mut resources,
                owner,
                VmExitReason::Killed,
            )
            .expect_err("ownership mismatch must fail closed"),
        format!(
            "process {} resource ownership graph mismatch: table [{}], memory []",
            owner.as_u64(),
            id.as_u64()
        )
    );
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );
    assert_eq!(resources.snapshots().len(), 1);
    assert_eq!(resources.snapshots()[0].id, id);
    assert_eq!(
        processes.get(owner).expect("owner").resource_handles.len(),
        1
    );
}

#[test]
pub(super) fn memory_logical_value_size_accounts_nested_structural_values_exactly() {
    for (value, expected) in [
        (ReplValue::Unit, 0),
        (ReplValue::Bool(true), 1),
        (ReplValue::Int(1), 8),
        (ReplValue::Float("1".to_string()), 17),
        (ReplValue::String("x".to_string()), 17),
        (ReplValue::Bytes(vec![0, 1, 2].into()), 19),
        (
            ReplValue::BitString(VmBitString::from_bytes([0xff], 3).expect("bitstring")),
            25,
        ),
        (ReplValue::Atom("x".to_string()), 17),
        (ReplValue::Type("T".to_string()), 17),
        (ReplValue::Tuple(vec![ReplValue::Int(1)]), 32),
        (ReplValue::List(vec![ReplValue::Int(1)]), 32),
        (ReplValue::Set(vec![ReplValue::Int(1)]), 32),
        (
            ReplValue::Iterator {
                items: vec![ReplValue::Int(1)],
                index: 0,
            },
            40,
        ),
        (
            ReplValue::Map(vec![(ReplValue::Int(1), ReplValue::Int(2))]),
            48,
        ),
    ] {
        assert_eq!(logical_value_bytes(&value), Ok(expected));
    }
    let value = ReplValue::Record {
        name: "User".to_string(),
        fields: vec![
            ("name".to_string(), ReplValue::String("Ada".to_string())),
            (
                "flags".to_string(),
                ReplValue::List(vec![ReplValue::Bool(true), ReplValue::Int(7)]),
            ),
        ],
    };

    assert_eq!(logical_value_bytes(&value), Ok(153));
}

#[test]
pub(super) fn memory_logical_value_size_handles_deep_nesting_without_recursive_host_calls() {
    let mut value = ReplValue::Int(1);
    for _ in 0..2_048 {
        value = ReplValue::List(vec![value]);
    }

    assert_eq!(logical_value_bytes(&value), Ok(49_160));
}

#[test]
pub(super) fn memory_logical_value_size_accounts_retained_achamp_patch_storage() {
    let entries = (0..128)
        .map(|index| (ReplValue::Int(index), ReplValue::String("v".to_string())))
        .collect::<Vec<_>>();
    let map = VmMapValue::from_entries(entries);
    let base = ReplValue::MapIndexed(map.clone());
    assert_eq!(logical_value_bytes(&base), Ok(5_264));

    let mut patched = map.clone();
    patched.insert_or_replace(ReplValue::Int(0), ReplValue::String("x".to_string()));
    assert_eq!(
        logical_value_bytes(&ReplValue::MapIndexed(patched)),
        Ok(5_305)
    );
}

#[test]
pub(super) fn memory_automatic_message_sizing_enforces_pressure_and_rejects_opaque_values() {
    let mut processes = VmProcessTable::default();
    let sender = processes.spawn_root(source());
    let recipient = processes.spawn_root(source());
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(100, 150).expect("limits"));
    let payload = ReplValue::List(vec![ReplValue::String("x".repeat(100))]);

    let first = memory
        .send_value_message(&mut processes, sender, recipient, payload)
        .expect("automatically sized payload");
    assert_eq!(first.published_message_id(), Some(1));
    assert_eq!(first.pressure.requested_bytes, 140);
    assert_eq!(
        first.pressure.outcome,
        VmMemoryPressureOutcome::SoftLimitExceeded
    );
    let rejected = memory
        .send_value_message(
            &mut processes,
            sender,
            recipient,
            ReplValue::Atom("too-large".to_string()),
        )
        .expect("hard pressure is typed");
    assert_eq!(rejected.published_message_id(), None);
    assert_eq!(
        rejected.pressure.outcome,
        VmMemoryPressureOutcome::HardLimitRejected
    );
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 140);
    assert_eq!(
        processes.get(recipient).expect("recipient").mailbox_len(),
        1
    );

    let opaque = ReplValue::RandomGenerator(crate::terlan_native::random::Generator::from_seed(7));
    assert_eq!(
        logical_value_bytes(&opaque),
        Err(VmValueSizeError::OpaqueValue {
            kind: "RandomGenerator"
        })
    );
    assert_eq!(
        memory
            .send_value_message(&mut processes, sender, recipient, opaque)
            .expect_err("opaque values require dedicated ownership"),
        "error[vm_memory_unaccounted_value]: `RandomGenerator` requires a dedicated ownership contract"
    );
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 140);
    assert_eq!(
        processes.get(recipient).expect("recipient").mailbox_len(),
        1
    );
}

#[test]
pub(super) fn memory_shared_allocation_retain_release_and_pressure_are_atomic() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source());
    let recipient = processes.spawn_root(source());
    let pressured = processes.spawn_root(source());
    let stranger = processes.spawn_root(source());
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(50, 100).expect("limits"));
    memory
        .account_heap(&mut processes, pressured, 70)
        .expect("pressured heap");
    let registered = memory
        .register_shared_allocation(
            &mut processes,
            owner,
            VmSharedAllocationKind::ProtocolBuffer,
            40,
        )
        .expect("register shared allocation");
    let id = registered.allocation_id.expect("allocation id");

    let retained = memory
        .retain_shared_allocation(&mut processes, id, owner, recipient)
        .expect("retain allocation");
    assert_eq!(retained.allocation_id, Some(id));
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 40);
    let duplicate = memory
        .retain_shared_allocation(&mut processes, id, owner, recipient)
        .expect("duplicate retain is idempotent");
    assert_eq!(duplicate.pressure.requested_bytes, 0);
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 40);
    let rejected = memory
        .retain_shared_allocation(&mut processes, id, owner, pressured)
        .expect("hard pressure is typed");
    assert_eq!(rejected.allocation_id, None);
    assert_eq!(
        rejected.pressure.outcome,
        VmMemoryPressureOutcome::HardLimitRejected
    );
    assert!(!memory
        .shared_allocation(id)
        .expect("allocation")
        .owners
        .contains(&pressured.as_u64()));
    assert_eq!(
        memory
            .retain_shared_allocation(&mut processes, id, stranger, pressured)
            .expect_err("non-owner cannot share allocation"),
        format!(
            "shared allocation {} is not owned by process {}",
            id.as_u64(),
            stranger.as_u64()
        )
    );

    assert!(!memory
        .release_shared_allocation(&mut processes, id, owner)
        .expect("first release"));
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    assert!(memory
        .release_shared_allocation(&mut processes, id, recipient)
        .expect("last release"));
    assert_eq!(processes.get(recipient).expect("recipient").heap_bytes, 0);
    assert_eq!(
        memory
            .release_shared_allocation(&mut processes, id, recipient)
            .expect_err("released allocation is stale"),
        format!("stale VM shared allocation {}", id.as_u64())
    );
}
