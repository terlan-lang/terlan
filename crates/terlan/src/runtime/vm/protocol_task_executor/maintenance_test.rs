use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn maintenance_deadlines_do_not_spin_replay_or_hide_callback_errors() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&calls);
    let mut task = VmProtocolMaintenance::new(Duration::from_secs(1), move || {
        observed.fetch_add(1, Ordering::SeqCst);
        Err(BoundaryError::message(
            ErrorDomain::VmRuntime,
            "fixture",
            "retry",
        ))
    })
    .unwrap();
    let first = task.next.unwrap();
    assert!(task.run_due(first).is_err());
    for _ in 0..5 {
        task.run_due(first).unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(task.timeout(first), Some(Duration::from_secs(1)));
    assert!(task.run_due(first + Duration::from_secs(1)).is_err());
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(task.run_due(first + Duration::from_secs(50)).is_err());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "missed ticks must not replay a burst"
    );
    assert_eq!(
        task.timeout(first + Duration::from_secs(50)),
        Some(Duration::from_secs(1))
    );
}

#[test]
fn maintenance_validates_intervals_and_combines_timer_deadlines() {
    for interval in [Duration::ZERO, Duration::MAX] {
        assert!(VmProtocolMaintenance::new(interval, || Ok(())).is_err());
    }
    let short = Some(Duration::from_millis(1));
    let long = Some(Duration::from_secs(1));
    assert_eq!(next_timeout(None, None), None);
    assert_eq!(next_timeout(None, short), short);
    assert_eq!(next_timeout(short, None), short);
    assert_eq!(next_timeout(short, long), short);
    assert_eq!(next_timeout(long, short), short);
}

#[test]
fn maintenance_deadline_overflow_disables_the_hook_without_busy_looping() {
    let mut task = VmProtocolMaintenance::new(Duration::from_secs(1), || {
        panic!("overflow must not invoke the callback")
    })
    .unwrap();
    let now = task.next.unwrap();
    task.interval = Duration::MAX;
    assert!(task
        .run_due(now)
        .unwrap_err()
        .to_string()
        .contains("deadline overflow"));
    assert_eq!(task.timeout(now), None);
    task.run_due(now).unwrap();
}

#[test]
fn idle_protocol_owner_runs_maintenance_without_sockets_and_stops_on_shutdown() {
    let (send, receive) = std::sync::mpsc::channel();
    let mut owner = idle_owner(
        VmProtocolMaintenance::new(Duration::from_millis(5), move || {
            assert!(super::super::CURRENT_PROTOCOL_SCHEDULER
                .with(|current| current.get())
                .is_some());
            let _ = send.send(());
            Ok(())
        })
        .unwrap(),
    );
    let first = receive.recv_timeout(Duration::from_secs(5));
    let second = receive.recv_timeout(Duration::from_secs(5));
    owner.stop().unwrap();
    assert!(
        first.is_ok() && second.is_ok(),
        "idle owner must wake for maintenance"
    );
    while receive.try_recv().is_ok() {}
    assert_eq!(
        receive.recv_timeout(Duration::from_millis(20)),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
    );
}

fn idle_owner(maintenance: VmProtocolMaintenance) -> super::super::server::VmProtocolTaskServer {
    use super::super::{
        server::VmProtocolTaskServer, VmProtocolCapacity, VmProtocolShardStartup,
        VmSchedulerTopology, CURRENT_PROTOCOL_SCHEDULER,
    };
    let scheduler = VmSchedulerTopology::new(1)
        .unwrap()
        .schedulers()
        .next()
        .unwrap();
    let mut startup = VmProtocolShardStartup::new(
        scheduler,
        Arc::new(|_, _| panic!("fixture has no socket admissions")),
        Arc::new(VmProtocolCapacity::default()),
    )
    .unwrap();
    let control = startup.control_port();
    startup.maintenance = Some(maintenance);
    let thread = std::thread::spawn(move || {
        CURRENT_PROTOCOL_SCHEDULER.with(|current| current.set(Some(scheduler)));
        startup.run()
    });
    VmProtocolTaskServer::new(vec![control], vec![thread])
}

#[test]
fn idle_owner_expires_real_package_sessions_without_requests_or_manual_ticks() {
    use crate::runtime::vm::actor_state::VmActorStateStore;
    use terlan_http_native::{
        session_registry::RecoveryPolicy, session_service::SessionService,
        session_store::SessionStore,
    };
    let service = SessionService::new(
        SessionStore::new(VmActorStateStore::default(), 1, RecoveryPolicy::FailClosed).unwrap(),
    );
    service.start_clock().unwrap();
    let image = service.native_services().unwrap();
    let identity = image
        .call("std.http.session.current", &["".into()])
        .unwrap();
    image
        .call(
            "std.http.session.set",
            &[identity.clone(), "key".into(), "value".into()],
        )
        .unwrap();
    assert_eq!(
        image.call("std.http.session.is_live", std::slice::from_ref(&identity)),
        Ok(true.into())
    );
    let maintained = service.clone();
    let (send, receive) = std::sync::mpsc::channel();
    let mut owner = idle_owner(
        VmProtocolMaintenance::new(Duration::from_millis(10), move || {
            let result = maintained.maintain(64);
            let _ = send.send(result);
            Ok(())
        })
        .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut reclaimed = false;
    while let Ok(result) = receive.recv_timeout(deadline.saturating_duration_since(Instant::now()))
    {
        result.unwrap();
        if service.with_storage(|state| state.is_empty()).unwrap() {
            reclaimed = true;
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
    }
    owner.stop().unwrap();
    assert!(
        reclaimed,
        "idle owner must reclaim sessions using monotonic wall time"
    );
    assert_eq!(
        image.call("std.http.session.is_live", std::slice::from_ref(&identity)),
        Ok(false.into())
    );
    assert!(image
        .call("std.http.session.get", &[identity, "key".into()])
        .is_err());
}
