
#[test]
fn memory_bulk_shared_release_validates_every_allocation_before_mutation() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source());
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(50, 100).expect("limits"));
    let first = memory
        .register_shared_allocation(
            &mut processes,
            owner,
            VmSharedAllocationKind::ProtocolBuffer,
            10,
        )
        .expect("first allocation")
        .allocation_id
        .expect("first id");
    let second = memory
        .register_shared_allocation(
            &mut processes,
            owner,
            VmSharedAllocationKind::ProtocolBuffer,
            15,
        )
        .expect("second allocation")
        .allocation_id
        .expect("second id");

    assert_eq!(
        memory
            .release_shared_allocations(&mut processes, &[first, VmSharedAllocationId(999)], owner,)
            .expect_err("stale allocation rejects the full release"),
        "stale VM shared allocation 999"
    );
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 25);
    assert!(memory.shared_allocation(first).is_some());
    assert!(memory.shared_allocation(second).is_some());
    assert!(memory
        .release_shared_allocations(&mut processes, &[first, second], owner)
        .is_ok_and(|released| released == 2));
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
}

#[test]
fn memory_process_exit_releases_shared_references_and_preserves_other_owners() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source());
    let survivor = processes.spawn_root(source());
    let mut resources = VmResourceTable::default();
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(100, 200).expect("limits"));
    let shared = memory
        .register_shared_allocation(
            &mut processes,
            owner,
            VmSharedAllocationKind::ResponseBuffer,
            25,
        )
        .expect("shared response buffer")
        .allocation_id
        .expect("shared id");
    memory
        .retain_shared_allocation(&mut processes, shared, owner, survivor)
        .expect("survivor retain");
    let exclusive = memory
        .register_shared_allocation(
            &mut processes,
            owner,
            VmSharedAllocationKind::TemplateOutput,
            15,
        )
        .expect("exclusive template output")
        .allocation_id
        .expect("exclusive id");

    let exit = memory
        .exit_process_with_memory_cleanup(
            &mut processes,
            &mut resources,
            owner,
            VmExitReason::Killed,
        )
        .expect("accounted process exit");

    assert!(memory.shared_allocation(exclusive).is_none());
    assert_eq!(exit.released_shared_allocations, vec![shared, exclusive]);
    assert_eq!(
        memory
            .shared_allocation(shared)
            .expect("surviving allocation")
            .owners,
        std::collections::BTreeSet::from([survivor.as_u64()])
    );
    assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    assert_eq!(processes.get(survivor).expect("survivor").heap_bytes, 25);
    let owner_metrics = memory.process_metrics(owner).expect("owner metrics");
    assert_eq!(owner_metrics.high_water_bytes, 40);
    assert_eq!(owner_metrics.released_bytes, 40);
}

#[test]
fn memory_accounting_soak_preserves_ownership_and_writes_benchmark_report() {
    const ITERATIONS: u64 = 10_000;
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source());
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(4_096, 8_192).expect("limits"));
    let mut accounted = 0u64;
    let mut soft = 0u64;
    let mut hard = 0u64;
    let started = Instant::now();

    for iteration in 0..ITERATIONS {
        let logical_bytes = if iteration % 10 == 0 {
            9_000
        } else {
            ((iteration * 257) % 8_192 + 1) as usize
        };
        let (decision, shared) = if iteration % 2 == 0 {
            (
                memory
                    .account_heap(&mut processes, owner, logical_bytes)
                    .expect("heap reservation"),
                None,
            )
        } else {
            let shared = memory
                .register_shared_allocation(
                    &mut processes,
                    owner,
                    VmSharedAllocationKind::ProtocolBuffer,
                    logical_bytes,
                )
                .expect("shared reservation");
            (shared.pressure, shared.allocation_id)
        };
        match decision.outcome {
            VmMemoryPressureOutcome::Accounted => accounted += 1,
            VmMemoryPressureOutcome::SoftLimitExceeded => soft += 1,
            VmMemoryPressureOutcome::HardLimitRejected => hard += 1,
        }
        if let Some(allocation) = shared {
            memory
                .release_shared_allocation(&mut processes, allocation, owner)
                .expect("shared release");
        } else if decision.outcome != VmMemoryPressureOutcome::HardLimitRejected {
            memory
                .release_heap(&mut processes, owner, logical_bytes)
                .expect("heap release");
        }
        assert_eq!(processes.get(owner).expect("owner").heap_bytes, 0);
    }

    let elapsed_nanos = started.elapsed().as_nanos().max(1) as u64;
    let metrics = memory.process_metrics(owner).expect("metrics");
    assert_eq!((accounted, soft, hard), (4_510, 4_490, ITERATIONS / 10));
    assert_eq!(metrics.current_bytes, 0);
    let report = serde_json::json!({
        "schema": "terlan-vm-memory-soak-report-v1",
        "iterations": ITERATIONS,
        "elapsedNanos": elapsed_nanos,
        "operationsPerSecond": ITERATIONS.saturating_mul(1_000_000_000) / elapsed_nanos,
        "accountedDecisions": accounted,
        "softLimitDecisions": soft,
        "hardLimitDecisions": hard,
        "highWaterBytes": metrics.high_water_bytes,
        "releasedBytes": metrics.released_bytes,
        "retainedBytes": metrics.current_bytes,
    });
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/quality/vm-memory-soak-report.json");
    std::fs::write(path, format!("{report:#}\n")).expect("write soak report");
}
