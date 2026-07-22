use super::{
    VmAcmeMode, VmAcmeRenewalActorState, VmAcmeRenewalRetryPolicy, VmAcmeWorkerAccessDecision,
    VmAcmeWorkerExecutionLane, VmAcmeWorkerRequest, VmAcmeWorkerRuntime, VmAcmeWorkerState,
    VmAcmeWorkerWake,
};
use crate::runtime::vm::process::{VmProcessId, VmProcessSource, VmProcessTable};
use crate::runtime::vm::scheduler::VmScheduler;
use crate::runtime::vm::support_bundle::{
    VmSupportBundleReplayRecorder, VmSupportBundleReplayResourceKind,
};
use crate::runtime::vm::timer::{VmTimerKind, VmTimerTable};

fn process_source(name: &str) -> VmProcessSource {
    VmProcessSource::new("std.vm.AcmeWorker", name, 0)
}

#[test]
fn vm_acme_renewal_retry_policy_is_typed_and_deterministic() {
    assert!(VmAcmeRenewalRetryPolicy::new(0, 10, 7).is_err());
    assert!(VmAcmeRenewalRetryPolicy::new(3, 0, 7).is_err());

    let policy = VmAcmeRenewalRetryPolicy::new(3, 10, 7).expect("policy");
    assert_eq!(policy.max_attempts, 3);
    assert_eq!(policy.base_delay_ticks, 10);
    assert_eq!(policy.jitter_seed, 7);
    assert!(policy.delay_for_attempt(0).is_err());
    assert!(policy.delay_for_attempt(4).is_err());

    let first = policy.delay_for_attempt(1).expect("first delay");
    let second = policy.delay_for_attempt(2).expect("second delay");
    let third = policy.delay_for_attempt(3).expect("third delay");
    let replay = VmAcmeRenewalRetryPolicy::new(3, 10, 7)
        .expect("replay policy")
        .delay_for_attempt(2)
        .expect("replay delay");

    assert!((10..20).contains(&first));
    assert!((20..30).contains(&second));
    assert!((40..50).contains(&third));
    assert_eq!(second, replay);
}

#[test]
fn vm_acme_worker_runs_http01_state_machine_without_network() {
    let owner = VmProcessId::from_raw_for_test(41);
    let mut runtime = VmAcmeWorkerRuntime::new();
    let request = VmAcmeWorkerRequest::new(
        "example.com",
        "account-1",
        "example.com/acme-cache",
        VmAcmeMode::Staging,
    );

    let worker = runtime.start_worker(owner, request).expect("start worker");
    assert_eq!(worker.as_u64(), 1);

    let challenge_wake = runtime
        .prepare_http01_challenge(worker, "token_1", "token_1.key")
        .expect("prepare challenge");
    assert_eq!(
        challenge_wake,
        VmAcmeWorkerWake::ChallengeReady { owner, worker }
    );

    let info = runtime.inspect_worker(worker).expect("inspect challenge");
    match info.state {
        VmAcmeWorkerState::ChallengeReady(challenge) => {
            assert_eq!(challenge.token, "token_1");
            assert_eq!(challenge.key_authorization, "token_1.key");
            assert_eq!(challenge.route, "/.well-known/acme-challenge/token_1");
        }
        other => panic!("unexpected state: {other:?}"),
    }

    runtime.start_issuance(worker).expect("start issuance");
    let cache_wake = runtime.begin_cache_write(worker, 7).expect("cache write");
    assert_eq!(
        cache_wake,
        VmAcmeWorkerWake::CacheWriteReady { owner, worker }
    );

    let terminal = runtime.complete_worker(worker).expect("complete");
    assert_eq!(terminal, VmAcmeWorkerWake::Terminal { owner, worker });
    runtime
        .schedule_renewal(worker, 1_900_000_000)
        .expect("schedule renewal");

    let info = runtime.inspect_worker(worker).expect("inspect complete");
    assert_eq!(
        info.state,
        VmAcmeWorkerState::RenewalScheduled {
            not_before_epoch_secs: 1_900_000_000
        }
    );
    assert!(info.closed);
    assert_eq!(info.event_count, 6);
}

#[test]
fn vm_acme_worker_rejects_invalid_inputs_and_cleans_up_owner_workers() {
    let owner = VmProcessId::from_raw_for_test(7);
    let other_owner = VmProcessId::from_raw_for_test(8);
    let mut runtime = VmAcmeWorkerRuntime::new();

    let invalid_domain = VmAcmeWorkerRequest::new(
        "https://example.com",
        "account-1",
        "cache",
        VmAcmeMode::Live,
    );
    assert!(runtime.start_worker(owner, invalid_domain).is_err());

    let worker = runtime
        .start_worker(
            owner,
            VmAcmeWorkerRequest::new("example.com", "account-1", "cache", VmAcmeMode::Live),
        )
        .expect("start owner worker");
    let other_worker = runtime
        .start_worker(
            other_owner,
            VmAcmeWorkerRequest::new("other.test", "account-2", "cache-2", VmAcmeMode::Staging),
        )
        .expect("start other worker");

    assert!(runtime
        .prepare_http01_challenge(worker, "bad/token", "key")
        .is_err());
    assert!(runtime.begin_cache_write(worker, 0).is_err());

    let wakes = runtime.shutdown_owner_workers(owner);
    assert_eq!(wakes, vec![VmAcmeWorkerWake::Terminal { owner, worker }]);
    let owner_info = runtime.inspect_worker(worker).expect("inspect owner");
    assert_eq!(owner_info.state, VmAcmeWorkerState::Shutdown);
    assert!(owner_info.closed);

    let other_info = runtime
        .inspect_worker(other_worker)
        .expect("inspect other owner");
    assert_eq!(other_info.state, VmAcmeWorkerState::Requested);
    assert!(!other_info.closed);

    let cancelled = runtime
        .cancel_worker(other_worker, "stale challenge")
        .expect("cancel other worker");
    assert_eq!(
        cancelled,
        VmAcmeWorkerWake::Terminal {
            owner: other_owner,
            worker: other_worker
        }
    );
    assert!(runtime.cancel_worker(other_worker, "again").is_err());
}

#[test]
fn vm_acme_worker_captures_support_bundle_replay_steps() {
    let owner = VmProcessId::from_raw_for_test(99);
    let mut runtime = VmAcmeWorkerRuntime::new();
    let mut recorder = VmSupportBundleReplayRecorder::new(1234);
    let worker = runtime
        .start_worker(
            owner,
            VmAcmeWorkerRequest::new("example.org", "acct", "cache-key", VmAcmeMode::Staging),
        )
        .expect("start worker");

    let requested = runtime
        .capture_support_bundle_step(worker, &mut recorder)
        .expect("capture requested");
    assert_eq!(requested.sequence, 1);
    assert_eq!(requested.process, owner);
    assert_eq!(
        requested.resource.kind,
        VmSupportBundleReplayResourceKind::AcmeWorker
    );
    assert_eq!(requested.resource.handle, "acme-worker:1");
    assert_eq!(requested.operation, "acme.worker.requested");
    assert!(requested.outcome.contains("domain=example.org"));
    assert!(requested.outcome.contains("mode=staging"));

    runtime
        .prepare_http01_challenge(worker, "token", "token.key")
        .expect("prepare challenge");
    let challenge = runtime
        .capture_support_bundle_step(worker, &mut recorder)
        .expect("capture challenge");
    assert_eq!(challenge.sequence, 2);
    assert_eq!(challenge.operation, "acme.worker.challenge_ready");

    let metadata = recorder.finish_bundle();
    assert_eq!(metadata.scheduler_seed, 1234);
    assert_eq!(metadata.steps.len(), 2);
    assert!(metadata.finished);
}

#[test]
fn vm_acme_worker_enforces_owner_backpressure_limit() {
    let owner = VmProcessId::from_raw_for_test(17);
    let other_owner = VmProcessId::from_raw_for_test(18);
    let mut runtime = VmAcmeWorkerRuntime::with_owner_limit(1).expect("limited runtime");

    let first = runtime
        .start_worker(
            owner,
            VmAcmeWorkerRequest::new("one.example", "acct", "cache-1", VmAcmeMode::Live),
        )
        .expect("first worker");

    let second_for_same_owner = runtime.start_worker(
        owner,
        VmAcmeWorkerRequest::new("two.example", "acct", "cache-2", VmAcmeMode::Live),
    );
    assert!(second_for_same_owner
        .expect_err("owner limit should reject")
        .contains("open worker limit"));

    runtime
        .start_worker(
            other_owner,
            VmAcmeWorkerRequest::new("other.example", "acct-2", "cache-3", VmAcmeMode::Staging),
        )
        .expect("other owner is independent");

    runtime
        .cancel_worker(first, "release queue slot")
        .expect("cancel first");
    runtime
        .start_worker(
            owner,
            VmAcmeWorkerRequest::new("two.example", "acct", "cache-2", VmAcmeMode::Live),
        )
        .expect("owner slot released after terminal state");

    assert!(VmAcmeWorkerRuntime::with_owner_limit(0).is_err());
}

#[test]
fn vm_acme_worker_emits_challenge_and_issuance_telemetry_spans() {
    let owner = VmProcessId::from_raw_for_test(23);
    let mut runtime = VmAcmeWorkerRuntime::new();
    let worker = runtime
        .start_worker(
            owner,
            VmAcmeWorkerRequest::new("telemetry.example", "acct", "cache", VmAcmeMode::Live),
        )
        .expect("start worker");

    runtime
        .prepare_http01_challenge(worker, "token", "token.key")
        .expect("prepare challenge");
    runtime.start_issuance(worker).expect("start issuance");
    runtime.begin_cache_write(worker, 1).expect("cache write");

    let spans = runtime.telemetry_spans(worker).expect("telemetry spans");
    let names: Vec<&str> = spans.iter().map(|span| span.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "acme.worker.request",
            "acme.challenge.ready",
            "acme.issuance.started",
            "acme.cache.write"
        ]
    );
    assert!(spans.iter().all(|span| span.worker == worker));
    assert!(spans.iter().all(|span| span.owner == owner));
    assert!(spans.iter().all(|span| span.domain == "telemetry.example"));
    assert!(spans.iter().all(|span| span.mode == VmAcmeMode::Live));
    assert!(spans.iter().all(|span| !span.terminal));

    runtime.complete_worker(worker).expect("complete");
    let spans = runtime.telemetry_spans(worker).expect("terminal span");
    let terminal = spans.last().expect("last telemetry span");
    assert_eq!(terminal.name, "acme.worker.completed");
    assert!(terminal.terminal);
}

#[test]
fn vm_acme_worker_authorizes_http01_challenge_route_through_policy_hook() {
    let owner = VmProcessId::from_raw_for_test(31);
    let mut runtime = VmAcmeWorkerRuntime::new();
    let worker = runtime
        .start_worker(
            owner,
            VmAcmeWorkerRequest::new("policy.example", "acct", "cache", VmAcmeMode::Staging),
        )
        .expect("start worker");

    assert_eq!(
        runtime
            .challenge_route_access_decision(worker, "GET", "/.well-known/acme-challenge/token")
            .expect("not ready decision"),
        VmAcmeWorkerAccessDecision::Deny {
            reason: "ACME HTTP-01 challenge is not ready".to_string()
        }
    );

    runtime
        .prepare_http01_challenge(worker, "token", "token.key")
        .expect("prepare challenge");

    assert_eq!(
        runtime
            .challenge_route_access_decision(worker, "GET", "/.well-known/acme-challenge/token")
            .expect("allow get"),
        VmAcmeWorkerAccessDecision::Allow {
            route: "/.well-known/acme-challenge/token".to_string()
        }
    );
    assert_eq!(
        runtime
            .challenge_route_access_decision(worker, "HEAD", "/.well-known/acme-challenge/token")
            .expect("allow head"),
        VmAcmeWorkerAccessDecision::Allow {
            route: "/.well-known/acme-challenge/token".to_string()
        }
    );
    assert!(matches!(
        runtime
            .challenge_route_access_decision(worker, "POST", "/.well-known/acme-challenge/token")
            .expect("deny post"),
        VmAcmeWorkerAccessDecision::Deny { reason } if reason.contains("method")
    ));
    assert!(matches!(
        runtime
            .challenge_route_access_decision(worker, "GET", "/wrong")
            .expect("deny wrong route"),
        VmAcmeWorkerAccessDecision::Deny { reason } if reason.contains("route")
    ));
}

#[test]
fn vm_acme_worker_parks_and_wakes_issuance_waiters() {
    let owner = VmProcessId::from_raw_for_test(44);
    let waiter = VmProcessId::from_raw_for_test(45);
    let mut runtime = VmAcmeWorkerRuntime::new();
    let worker = runtime
        .start_worker(
            owner,
            VmAcmeWorkerRequest::new("scheduler.example", "acct", "cache", VmAcmeMode::Live),
        )
        .expect("start worker");

    runtime
        .park_issuance_waiter(worker, waiter)
        .expect("park waiter");
    runtime
        .park_issuance_waiter(worker, waiter)
        .expect("duplicate park is idempotent");
    runtime
        .prepare_http01_challenge(worker, "token", "token.key")
        .expect("prepare challenge");

    let wakes = runtime.start_issuance(worker).expect("start issuance");
    assert_eq!(
        wakes,
        vec![VmAcmeWorkerWake::IssuanceReady {
            process: waiter,
            worker
        }]
    );
    assert!(runtime.park_issuance_waiter(worker, waiter).is_err());

    runtime
        .begin_cache_write(worker, 1)
        .expect("begin cache write");
    runtime.complete_worker(worker).expect("complete");
    assert!(runtime.park_issuance_waiter(worker, waiter).is_err());
}

#[test]
fn vm_acme_worker_emits_due_renewal_wakeups() {
    let owner = VmProcessId::from_raw_for_test(51);
    let mut runtime = VmAcmeWorkerRuntime::new();
    let worker = runtime
        .start_worker(
            owner,
            VmAcmeWorkerRequest::new("renew.example", "acct", "cache", VmAcmeMode::Live),
        )
        .expect("start worker");

    runtime
        .prepare_http01_challenge(worker, "token", "token.key")
        .expect("prepare challenge");
    runtime.start_issuance(worker).expect("start issuance");
    runtime.begin_cache_write(worker, 1).expect("cache write");
    runtime.complete_worker(worker).expect("complete");
    runtime
        .schedule_renewal(worker, 2_000)
        .expect("schedule renewal");

    assert!(runtime.renewal_due_wakeups(1_999).is_empty());
    assert_eq!(
        runtime.renewal_due_wakeups(2_000),
        vec![VmAcmeWorkerWake::RenewalDue {
            owner,
            worker,
            not_before_epoch_secs: 2_000
        }]
    );
    assert_eq!(
        runtime.renewal_due_wakeups(2_100),
        vec![VmAcmeWorkerWake::RenewalDue {
            owner,
            worker,
            not_before_epoch_secs: 2_000
        }]
    );
}

#[test]
fn vm_acme_worker_schedules_renewal_through_vm_timer_table() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(process_source("renewal_worker"));
    let mut timers = VmTimerTable::default();
    let mut runtime = VmAcmeWorkerRuntime::new();
    let worker = runtime
        .start_worker(
            owner,
            VmAcmeWorkerRequest::new("timer.example", "acct", "cache", VmAcmeMode::Live),
        )
        .expect("start worker");

    runtime
        .prepare_http01_challenge(worker, "token", "token.key")
        .expect("prepare challenge");
    runtime.start_issuance(worker).expect("start issuance");
    runtime.begin_cache_write(worker, 1).expect("cache write");
    runtime.complete_worker(worker).expect("complete");

    let timer = runtime
        .schedule_renewal_timer(worker, &processes, &mut timers, 4_200)
        .expect("schedule renewal timer");

    let snapshots = timers.snapshots();
    assert_eq!(timer.as_u64(), 1);
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].id, timer);
    assert_eq!(snapshots[0].owner, owner);
    assert_eq!(snapshots[0].deadline_tick, 4_200);
    assert_eq!(snapshots[0].kind, VmTimerKind::OneShot);
    assert_eq!(
        runtime.renewal_due_wakeups(4_200),
        vec![VmAcmeWorkerWake::RenewalDue {
            owner,
            worker,
            not_before_epoch_secs: 4_200
        }]
    );
}

#[test]
fn vm_acme_renewal_actor_owns_worker_timer_and_shutdown_cleanup() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(process_source("renewal_actor"));
    let mut timers = VmTimerTable::default();
    let mut runtime = VmAcmeWorkerRuntime::new();
    let worker = runtime
        .start_worker(
            owner,
            VmAcmeWorkerRequest::new("actor.example", "acct", "cache", VmAcmeMode::Live),
        )
        .expect("start worker");

    runtime
        .prepare_http01_challenge(worker, "token", "token.key")
        .expect("prepare challenge");
    runtime.start_issuance(worker).expect("start issuance");
    runtime.begin_cache_write(worker, 1).expect("cache write");
    runtime.complete_worker(worker).expect("complete");

    let mut actor = runtime
        .spawn_renewal_actor(&mut processes, &mut timers, worker, 8_000)
        .expect("spawn renewal actor");

    assert_eq!(actor.owner, owner);
    assert_eq!(actor.worker, worker);
    assert_eq!(actor.state, VmAcmeRenewalActorState::Waiting);
    assert_eq!(actor.renewal_timer.expect("actor timer").as_u64(), 1);
    assert_eq!(timers.snapshots().len(), 1);
    assert!(processes
        .get(owner)
        .expect("owner process")
        .resource_handles
        .contains(&"acme-renewal-actor:1".to_string()));

    assert!(actor.begin_due_renewal(&mut runtime, 7_999).is_err());
    let mut scheduler = VmScheduler::default();
    let timer_events = timers.advance_clock(&mut processes, &mut scheduler, 8_000);
    assert_eq!(timer_events.len(), 1);
    assert_eq!(
        actor
            .begin_due_renewal(&mut runtime, 8_000)
            .expect("due actor renewal"),
        VmAcmeWorkerWake::RenewalDue {
            owner,
            worker,
            not_before_epoch_secs: 8_000
        }
    );
    assert_eq!(actor.state, VmAcmeRenewalActorState::Renewing);
    assert!(actor.renewal_timer.is_none());
    assert!(timers.snapshots().is_empty());

    let shutdown = actor
        .shutdown(&mut runtime, &mut timers, &mut processes)
        .expect("shutdown renewal actor");
    assert!(shutdown.cancelled_timer.is_none());
    assert_eq!(
        shutdown.terminal_wake,
        VmAcmeWorkerWake::Terminal { owner, worker }
    );
    assert_eq!(actor.state, VmAcmeRenewalActorState::Shutdown);
    assert!(!processes
        .get(owner)
        .expect("owner process")
        .resource_handles
        .contains(&"acme-renewal-actor:1".to_string()));
    assert!(actor
        .shutdown(&mut runtime, &mut timers, &mut processes)
        .expect_err("double shutdown rejected")
        .contains("already shutdown"));

    let worker_info = runtime.inspect_worker(worker).expect("inspect worker");
    assert_eq!(worker_info.state, VmAcmeWorkerState::Shutdown);
    assert!(worker_info.closed);
}

#[test]
fn vm_acme_worker_denies_stale_challenge_access_after_renewal_scheduled() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(process_source("renewal_access_policy"));
    let mut timers = VmTimerTable::default();
    let mut runtime = VmAcmeWorkerRuntime::new();
    let worker = runtime
        .start_worker(
            owner,
            VmAcmeWorkerRequest::new("access.example", "acct", "cache", VmAcmeMode::Live),
        )
        .expect("start worker");

    runtime
        .prepare_http01_challenge(worker, "token", "token.key")
        .expect("prepare challenge");
    assert_eq!(
        runtime
            .challenge_route_access_decision(worker, "GET", "/.well-known/acme-challenge/token")
            .expect("challenge route should be available before issuance"),
        VmAcmeWorkerAccessDecision::Allow {
            route: "/.well-known/acme-challenge/token".to_string()
        }
    );
    runtime.start_issuance(worker).expect("start issuance");
    runtime.begin_cache_write(worker, 1).expect("cache write");
    runtime.complete_worker(worker).expect("complete");
    runtime
        .schedule_renewal_timer(worker, &processes, &mut timers, 4_200)
        .expect("schedule renewal timer");

    assert_eq!(
        runtime
            .challenge_route_access_decision(worker, "GET", "/.well-known/acme-challenge/token")
            .expect("stale challenge route should deny after renewal scheduling"),
        VmAcmeWorkerAccessDecision::Deny {
            reason: "ACME HTTP-01 challenge is not ready".to_string()
        }
    );
}

#[test]
fn vm_acme_worker_records_renewal_telemetry_and_redacted_support_bundle_step() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(process_source("renewal_telemetry"));
    let mut timers = VmTimerTable::default();
    let mut runtime = VmAcmeWorkerRuntime::new();
    let worker = runtime
        .start_worker(
            owner,
            VmAcmeWorkerRequest::new(
                "telemetry.example",
                "account-secret",
                "cache-secret",
                VmAcmeMode::Live,
            ),
        )
        .expect("start worker");

    runtime
        .prepare_http01_challenge(worker, "token", "token.key")
        .expect("prepare challenge");
    runtime.start_issuance(worker).expect("start issuance");
    runtime.begin_cache_write(worker, 1).expect("cache write");
    runtime.complete_worker(worker).expect("complete");
    runtime
        .schedule_renewal_timer(worker, &processes, &mut timers, 4_200)
        .expect("schedule renewal timer");

    let spans = runtime.telemetry_spans(worker).expect("telemetry spans");
    let renewal_span = spans.last().expect("renewal telemetry span");
    assert_eq!(renewal_span.name, "acme.renewal.scheduled");
    assert_eq!(renewal_span.worker, worker);
    assert_eq!(renewal_span.owner, owner);
    assert_eq!(renewal_span.domain, "telemetry.example");
    assert_eq!(renewal_span.mode, VmAcmeMode::Live);
    assert!(!renewal_span.terminal);

    let mut recorder = VmSupportBundleReplayRecorder::new(777);
    let step = runtime
        .capture_support_bundle_step(worker, &mut recorder)
        .expect("capture renewal support bundle step");
    assert_eq!(step.operation, "acme.worker.renewal_scheduled");
    assert!(step.outcome.contains("domain=telemetry.example"));
    assert!(step.outcome.contains("account=<redacted>"));
    assert!(step.outcome.contains("cache_key=<redacted>"));
    assert!(step.outcome.contains("mode=live"));
    assert!(!step.outcome.contains("account-secret"));
    assert!(!step.outcome.contains("cache-secret"));
}

#[test]
fn vm_acme_worker_routes_challenge_after_due_renewal_begins() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(process_source("renewal_challenge"));
    let mut timers = VmTimerTable::default();
    let mut runtime = VmAcmeWorkerRuntime::new();
    let worker = runtime
        .start_worker(
            owner,
            VmAcmeWorkerRequest::new("renew-route.example", "acct", "cache", VmAcmeMode::Live),
        )
        .expect("start worker");

    runtime
        .prepare_http01_challenge(worker, "initial", "initial.key")
        .expect("prepare initial challenge");
    runtime.start_issuance(worker).expect("start issuance");
    runtime.begin_cache_write(worker, 1).expect("cache write");
    runtime.complete_worker(worker).expect("complete");
    runtime
        .schedule_renewal_timer(worker, &processes, &mut timers, 5_000)
        .expect("schedule renewal timer");

    assert!(runtime.begin_due_renewal(worker, 4_999).is_err());
    assert_eq!(
        runtime
            .begin_due_renewal(worker, 5_000)
            .expect("begin due renewal"),
        VmAcmeWorkerWake::RenewalDue {
            owner,
            worker,
            not_before_epoch_secs: 5_000
        }
    );
    runtime
        .prepare_http01_challenge(worker, "renewal", "renewal.key")
        .expect("prepare renewal challenge");
    assert_eq!(
        runtime
            .challenge_route_access_decision(worker, "GET", "/.well-known/acme-challenge/renewal")
            .expect("renewal challenge route should be available"),
        VmAcmeWorkerAccessDecision::Allow {
            route: "/.well-known/acme-challenge/renewal".to_string()
        }
    );
}

#[test]
fn vm_acme_worker_captures_deterministic_renewal_cache_tls_handoff_replay() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(process_source("renewal_replay"));
    let mut timers = VmTimerTable::default();
    let mut runtime = VmAcmeWorkerRuntime::new();
    let worker = runtime
        .start_worker_for_lane(
            owner,
            VmAcmeWorkerRequest::new(
                "replay.example",
                "account-secret",
                "cache-secret",
                VmAcmeMode::Live,
            ),
            VmAcmeWorkerExecutionLane::DeterministicFixture {
                fixture_id: "renewal-fixture-1".to_string(),
            },
        )
        .expect("start deterministic fixture worker");

    runtime
        .prepare_http01_challenge(worker, "token", "token.key")
        .expect("prepare challenge");
    runtime.start_issuance(worker).expect("start issuance");
    runtime.begin_cache_write(worker, 9).expect("cache write");
    runtime.complete_worker(worker).expect("complete");
    runtime
        .schedule_renewal_timer(worker, &processes, &mut timers, 6_400)
        .expect("schedule renewal timer");

    let mut recorder = VmSupportBundleReplayRecorder::new(4242);
    let steps = runtime
        .capture_deterministic_renewal_cache_tls_handoff_replay(
            worker,
            &mut recorder,
            "listener-https",
        )
        .expect("capture deterministic replay");

    let operations: Vec<&str> = steps.iter().map(|step| step.operation.as_str()).collect();
    assert_eq!(
        operations,
        vec![
            "acme.renewal.replay.metadata",
            "acme.renewal.replay.cache_handoff",
            "acme.renewal.replay.tls_handoff"
        ]
    );
    assert_eq!(steps[0].sequence, 1);
    assert_eq!(steps[1].sequence, 2);
    assert_eq!(steps[2].sequence, 3);
    assert_eq!(
        steps[2].resource.kind,
        VmSupportBundleReplayResourceKind::TlsConnection
    );
    assert_eq!(steps[2].resource.handle, "tls-listener:listener-https");

    let replay = recorder.finish_bundle();
    assert_eq!(replay.scheduler_seed, 4242);
    assert_eq!(replay.steps.len(), 3);
    assert!(replay.finished);
    let replay_text = format!("{:?}", replay.steps);
    assert!(replay_text.contains("renewal-fixture-1"));
    assert!(replay_text.contains("cache_key=<redacted>"));
    assert!(replay_text.contains("old_connections_preserved=true"));
    assert!(!replay_text.contains("account-secret"));
    assert!(!replay_text.contains("cache-secret"));

    let live_worker = runtime
        .start_worker_for_lane(
            owner,
            VmAcmeWorkerRequest::new("live-replay.example", "acct", "cache", VmAcmeMode::Live),
            VmAcmeWorkerExecutionLane::Live {
                directory_url: "https://acme-v02.api.letsencrypt.org/directory".to_string(),
            },
        )
        .expect("start live worker");
    runtime
        .prepare_http01_challenge(live_worker, "live", "live.key")
        .expect("prepare live challenge");
    runtime
        .start_issuance(live_worker)
        .expect("start live issuance");
    runtime
        .begin_cache_write(live_worker, 1)
        .expect("live cache write");
    runtime.complete_worker(live_worker).expect("complete live");
    runtime
        .schedule_renewal(live_worker, 7_000)
        .expect("schedule live renewal");
    assert!(runtime
        .capture_deterministic_renewal_cache_tls_handoff_replay(
            live_worker,
            &mut VmSupportBundleReplayRecorder::new(1),
            "listener-https",
        )
        .expect_err("live lane cannot use deterministic replay")
        .contains("deterministic fixture lane"));
}

#[test]
fn vm_acme_worker_uses_one_contract_for_fixture_and_live_lanes() {
    let owner = VmProcessId::from_raw_for_test(61);
    let mut runtime = VmAcmeWorkerRuntime::new();
    let fixture_worker = runtime
        .start_worker_for_lane(
            owner,
            VmAcmeWorkerRequest::new(
                "fixture.example",
                "acct",
                "fixture-cache",
                VmAcmeMode::Staging,
            ),
            VmAcmeWorkerExecutionLane::DeterministicFixture {
                fixture_id: "fixture-1".to_string(),
            },
        )
        .expect("fixture worker");
    let live_worker = runtime
        .start_worker_for_lane(
            owner,
            VmAcmeWorkerRequest::new("live.example", "acct", "live-cache", VmAcmeMode::Live),
            VmAcmeWorkerExecutionLane::Live {
                directory_url: "https://acme-v02.api.letsencrypt.org/directory".to_string(),
            },
        )
        .expect("live worker");

    let fixture = runtime
        .inspect_worker(fixture_worker)
        .expect("inspect fixture");
    let live = runtime.inspect_worker(live_worker).expect("inspect live");
    assert_eq!(fixture.state, VmAcmeWorkerState::Requested);
    assert_eq!(live.state, VmAcmeWorkerState::Requested);
    assert_eq!(
        fixture.execution_lane,
        VmAcmeWorkerExecutionLane::DeterministicFixture {
            fixture_id: "fixture-1".to_string()
        }
    );
    assert_eq!(
        live.execution_lane,
        VmAcmeWorkerExecutionLane::Live {
            directory_url: "https://acme-v02.api.letsencrypt.org/directory".to_string()
        }
    );

    let mut recorder = VmSupportBundleReplayRecorder::new(9001);
    let fixture_step = runtime
        .capture_support_bundle_step(fixture_worker, &mut recorder)
        .expect("fixture support bundle");
    let live_step = runtime
        .capture_support_bundle_step(live_worker, &mut recorder)
        .expect("live support bundle");
    assert!(fixture_step.outcome.contains("lane=deterministic-fixture"));
    assert!(live_step.outcome.contains("lane=live"));

    assert!(runtime
        .start_worker_for_lane(
            owner,
            VmAcmeWorkerRequest::new("bad.example", "acct", "cache", VmAcmeMode::Live),
            VmAcmeWorkerExecutionLane::Live {
                directory_url: "http://insecure.test/directory".to_string()
            },
        )
        .is_err());
}

#[test]
fn vm_acme_worker_starts_issuance_without_new_challenge_for_valid_authorizations() {
    let owner = VmProcessId::from_raw_for_test(62);
    let mut runtime = VmAcmeWorkerRuntime::new();
    let worker = runtime
        .start_worker_for_lane(
            owner,
            VmAcmeWorkerRequest::new("valid.example", "acct", "cache", VmAcmeMode::Live),
            VmAcmeWorkerExecutionLane::Live {
                directory_url: "https://acme-v02.api.letsencrypt.org/directory".to_string(),
            },
        )
        .expect("live worker");

    let wakes = runtime.start_issuance(worker).expect("start issuance");
    let info = runtime.inspect_worker(worker).expect("inspect worker");

    assert!(wakes.is_empty());
    assert_eq!(info.state, VmAcmeWorkerState::Issuing);
}
