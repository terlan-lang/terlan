use super::*;
use crate::runtime::vm::process::{VmExitReason, VmProcessSource};
use crate::terlan_native::postgres;

#[cfg(all(feature = "postgres-libpq", not(feature = "serve-runtime-bin")))]
fn connect(runtime: &mut VmActorRuntime, owner: VmProcessId) {
    let config = VmPostgresConnectConfig::new(postgres::Config::new(
        "postgres://control-test@localhost/database",
    ))
    .expect("valid lazy Postgres config");
    let request = runtime
        .postgres_connect(
            owner,
            config,
            crate::runtime::vm::postgres::VmPostgresDeadline {
                now_tick: 0,
                timeout_ticks: 10,
            },
        )
        .expect("submit lazy pool registration");
    assert_eq!(
        runtime
            .drive_postgres_once()
            .expect("drive lazy pool registration"),
        Some(request)
    );
    assert!(matches!(
        runtime
            .take_postgres_reply(owner, request)
            .expect("take pool reply"),
        VmPostgresReply::Pool(_)
    ));
}

#[cfg(all(feature = "postgres-libpq", not(feature = "serve-runtime-bin")))]
#[test]
fn postgres_control_drain_isolates_failure_between_actor_executions() {
    let mut runtime = VmActorRuntime::default();
    let first = runtime.spawn_root(VmProcessSource::new("app.First", "run", 0));
    let second = runtime.spawn_root(VmProcessSource::new("app.Second", "run", 0));
    connect(&mut runtime, first);
    connect(&mut runtime, second);

    runtime
        .exit_actor(first, VmExitReason::Killed)
        .expect("exit first actor");
    runtime
        .exit_actor(second, VmExitReason::Killed)
        .expect("exit second actor");
    let already_closed = runtime
        .postgres_controls
        .pop_front()
        .expect("first actor emitted pool cleanup");
    assert!(matches!(
        already_closed,
        VmPostgresDriverControl::ClosePool { .. }
    ));
    runtime
        .postgres_driver
        .apply_control(already_closed)
        .expect("close first actor pool once");
    let independently_open = *runtime
        .postgres_controls
        .front()
        .expect("second actor emitted independent pool cleanup");
    runtime.postgres_controls.push_front(already_closed);

    let error = runtime
        .drive_postgres_controls()
        .expect_err("stale first control must remain observable");
    assert!(error.contains("postgres.driver.stale_resource"));
    assert!(runtime.postgres_controls.is_empty());
    assert!(runtime
        .postgres_driver
        .apply_control(independently_open)
        .is_err());
}

#[cfg(any(feature = "serve-runtime-bin", not(feature = "postgres-libpq")))]
#[test]
fn postgres_worker_rejections_are_delivered_to_the_requesting_actor() {
    let mut runtime = VmActorRuntime::default();
    let first = runtime.spawn_root(VmProcessSource::new("app.First", "run", 0));
    let second = runtime.spawn_root(VmProcessSource::new("app.Second", "run", 0));
    for owner in [first, second] {
        let request = runtime
            .postgres_connect(
                owner,
                VmPostgresConnectConfig::new(postgres::Config::new(
                    "postgres://secret:never-print@localhost/database",
                ))
                .unwrap(),
                VmPostgresDeadline {
                    now_tick: 0,
                    timeout_ticks: 10,
                },
            )
            .unwrap();
        assert_eq!(runtime.drive_postgres_once().unwrap(), Some(request));
        let other = if owner == first { second } else { first };
        assert!(runtime.take_postgres_reply(other, request).is_err());
        let VmPostgresReply::Error(failure) = runtime.take_postgres_reply(owner, request).unwrap()
        else {
            panic!("in-process Postgres must be rejected");
        };
        assert_eq!(failure.code, "postgres.capability_worker.required");
        assert!(!failure.message.contains("never-print"));
        runtime.exit_actor(owner, VmExitReason::Killed).unwrap();
    }
    assert_eq!(runtime.drive_postgres_once().unwrap(), None);
    assert!(runtime.postgres_controls.is_empty());
}

#[cfg(any(feature = "serve-runtime-bin", not(feature = "postgres-libpq")))]
#[test]
fn postgres_control_drain_rejects_and_consumes_every_unsupported_control() {
    let mut runtime = VmActorRuntime::default();
    for value in [1, 2] {
        runtime
            .postgres_controls
            .push_back(VmPostgresDriverControl::Cancel(RequestId { value }));
    }
    let error = runtime
        .drive_postgres_controls()
        .expect_err("worker transport is required");
    assert!(error.contains("postgres.capability_worker.required"));
    assert!(runtime.postgres_controls.is_empty());
    assert_eq!(runtime.drive_postgres_controls().unwrap(), 0);
}
