use super::super::super::process::{VmExitReason, VmProcessSource, VmProcessTable};
use super::super::super::table::{VmTableAccess, VmTableStore};
use super::super::{VmScheduler, VmSchedulerConfig, VmSchedulerDecision, VmSchedulerOutcome};
use super::{VmContentionCategory, VmContentionError, VmContentionTelemetry};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.ContentionParity", name, 0)
}

fn registered_processes(processes: &VmProcessTable) -> Vec<(String, String)> {
    processes
        .registered_names()
        .into_iter()
        .filter_map(|name| {
            processes
                .lookup_name(&name)
                .map(|pid| (format!("process:{}:{name}", pid.as_u64()), name.to_string()))
        })
        .collect()
}

fn registered_tables(tables: &VmTableStore) -> Vec<(String, String)> {
    tables
        .snapshots()
        .into_iter()
        .filter(|table| !table.name.trim().is_empty())
        .map(|table| (format!("table:{}", table.id.as_u64()), table.name))
        .collect()
}

#[test]
fn lcnt_suite_category_controls_are_transactional_and_complete() {
    let mut telemetry = VmContentionTelemetry::default();
    for (index, category) in VmContentionCategory::ALL.into_iter().enumerate() {
        telemetry
            .register_resource(
                category,
                format!("resource-{index}"),
                format!("{}-owner", category.control_name()),
            )
            .expect("stable VM resource");
    }

    for category in VmContentionCategory::ALL {
        telemetry
            .configure(&[category.control_name()])
            .expect("known category");
        assert_eq!(telemetry.enabled_categories(), vec![category]);
        let snapshot = telemetry.snapshot();
        assert_eq!(snapshot.records.len(), 1);
        assert_eq!(snapshot.records[0].category, category);
    }

    let before = telemetry.enabled_categories();
    assert_eq!(
        telemetry.configure(&["scheduler", "q_invalid"]),
        Err(VmContentionError::InvalidCategory("q_invalid".into()))
    );
    assert_eq!(telemetry.enabled_categories(), before);

    telemetry.configure(&[]).expect("disable telemetry");
    assert!(telemetry.snapshot().records.is_empty());
    assert!(!telemetry.retain_retired());

    let mut bounded = VmContentionTelemetry::with_capacity(1);
    bounded
        .register_resource(VmContentionCategory::Runtime, "first", "first")
        .expect("first bounded record");
    assert_eq!(
        bounded.register_resource(VmContentionCategory::Runtime, "second", "second"),
        Err(VmContentionError::CapacityExceeded { capacity: 1 })
    );
    bounded
        .configure(&["scheduler"])
        .expect("enable scheduler telemetry");
    bounded.observe_scheduler_wait(9, 3);
    assert_eq!(bounded.snapshot().dropped_records, 1);
}

#[test]
fn lcnt_suite_preserves_retired_resources_and_stable_registered_names() {
    let mut processes = VmProcessTable::default();
    let actor = processes.spawn_root(source("code_server"));
    processes
        .register_name("code_server", actor)
        .expect("registered actor");
    let mut tables = VmTableStore::default();
    let created = tables
        .create(&processes, actor, "code", VmTableAccess::OwnerOnly)
        .expect("registered table");
    let table = match created {
        super::super::super::table::VmTableEvent::Created { id, .. } => id,
        other => panic!("unexpected table event: {other:?}"),
    };

    let mut telemetry = VmContentionTelemetry::default();
    telemetry
        .configure(&["process", "db"])
        .expect("portable categories");
    telemetry
        .synchronize_registered_resources(
            registered_processes(&processes),
            registered_tables(&tables),
        )
        .expect("initial catalog");
    let live = telemetry.snapshot();
    assert_eq!(
        live.records
            .iter()
            .map(|record| (record.category, record.label.as_str(), record.active))
            .collect::<Vec<_>>(),
        vec![
            (VmContentionCategory::Table, "code", true),
            (VmContentionCategory::Process, "code_server", true),
        ]
    );

    telemetry.set_retain_retired(true);
    assert!(telemetry.retain_retired());
    for index in 0..1_000 {
        telemetry
            .register_resource(
                VmContentionCategory::Process,
                format!("transient:{index}"),
                format!("transient-{index}"),
            )
            .expect("bounded transient process resource");
    }
    processes
        .exit_process(actor, VmExitReason::Normal)
        .expect("actor exits");
    assert_eq!(tables.cleanup_owner(actor).len(), 1);
    telemetry
        .synchronize_registered_resources(
            registered_processes(&processes),
            registered_tables(&tables),
        )
        .expect("retired catalog");
    let retired = telemetry.snapshot();
    assert_eq!(retired.records.len(), 1_002);
    assert!(retired.records.iter().all(|record| !record.active));
    assert!(retired
        .records
        .iter()
        .any(|record| record.identity == format!("table:{}", table.as_u64())));

    telemetry.clear();
    assert!(telemetry.snapshot().records.is_empty());
}

#[test]
fn lcnt_suite_scheduler_wait_accounting_is_owner_local_and_logical() {
    let mut processes = VmProcessTable::default();
    let first = processes.spawn_root(source("first"));
    let second = processes.spawn_root(source("second"));
    let mut scheduler = VmScheduler::new(VmSchedulerConfig::new(10, 8));
    scheduler
        .contention_telemetry_mut()
        .configure(&["scheduler"])
        .expect("scheduler category");
    scheduler.enqueue_runnable(&processes, first).unwrap();
    scheduler.enqueue_runnable(&processes, second).unwrap();

    for expected in [first, second] {
        let run = scheduler
            .run_next(&mut processes, |_, _| VmSchedulerDecision::Block {
                reductions: 1,
            })
            .expect("scheduler run");
        assert_eq!(run.pid, Some(expected));
        assert_eq!(run.outcome, VmSchedulerOutcome::Blocked);
    }

    let snapshot = scheduler.contention_telemetry().snapshot();
    assert_eq!(snapshot.records.len(), 2);
    assert_eq!(
        snapshot
            .records
            .iter()
            .map(|record| record.acquisitions)
            .sum::<u64>(),
        2
    );
    assert_eq!(
        snapshot
            .records
            .iter()
            .map(|record| record.contentions)
            .sum::<u64>(),
        2
    );
    assert_eq!(
        snapshot
            .records
            .iter()
            .map(|record| record.total_wait_ticks)
            .sum::<u64>(),
        3
    );
    assert_eq!(
        snapshot
            .records
            .iter()
            .map(|record| record.max_wait_ticks)
            .max(),
        Some(2)
    );
}
