use super::super::super::process::{
    VmExitReason, VmProcessResumeState, VmProcessSource, VmProcessState,
};
use super::super::super::scheduler::{VmSchedulerDecision, VmSchedulerOutcome};
use super::super::super::ReplValue;
use super::super::{VmActorReceive, VmActorRuntime};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

fn assert_down_message(
    runtime: &mut VmActorRuntime,
    observer: super::super::VmProcessId,
    monitor_ref: u64,
    target: super::super::VmProcessId,
    reason: ReplValue,
) {
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(observer)
        .expect("receive monitor completion")
    else {
        panic!("monitor completion must be available");
    };
    assert_eq!(
        message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(monitor_ref as i64),
            ReplValue::Int(target.as_u64() as i64),
            reason,
        ])
    );
}

#[test]
fn normal_exit_atomically_cleans_owned_lifecycle_state_and_notifies_monitor() {
    let mut runtime = VmActorRuntime::default();
    let target = runtime.spawn_root(source("target"));
    let observer = runtime.spawn_root(source("observer"));
    let neighbor = runtime.spawn_root(source("neighbor"));
    runtime
        .register_name("target.primary", target)
        .expect("register primary name");
    runtime
        .register_name("target.secondary", target)
        .expect("register secondary name");
    let target_alias = runtime.create_alias(target).expect("target alias");
    let neighbor_alias = runtime.create_alias(neighbor).expect("neighbor alias");
    runtime
        .processes
        .get_mut(target)
        .expect("target process")
        .add_resource_handle("socket:target");
    runtime
        .processes
        .get_mut(target)
        .expect("target process")
        .add_resource_handle("file:target");
    runtime
        .send(
            neighbor,
            target,
            ReplValue::String("release mailbox memory".to_string()),
        )
        .expect("queue target message");
    let target_timer = runtime
        .send_after(target, neighbor, ReplValue::Int(1), 0, 50)
        .expect("start target-owned timer");
    let monitor_ref = runtime
        .monitor_actor(observer, target)
        .expect("monitor target");
    runtime
        .link_actors(target, neighbor)
        .expect("link neighbor");
    assert_eq!(
        runtime
            .receive_next_or_block(observer)
            .expect("block observer"),
        VmActorReceive::Blocked
    );

    let cleanup = runtime
        .exit_actor(target, VmExitReason::Normal)
        .expect("exit target");

    assert_eq!(
        cleanup,
        vec!["socket:target".to_string(), "file:target".to_string()]
    );
    let target_snapshot = runtime
        .processes()
        .snapshot(target)
        .expect("target snapshot");
    assert_eq!(
        target_snapshot.state,
        VmProcessState::Exited(VmExitReason::Normal)
    );
    assert_eq!(target_snapshot.mailbox_messages, 0);
    assert_eq!(target_snapshot.heap_bytes, 0);
    assert!(target_snapshot.resource_handles.is_empty());
    assert!(target_snapshot.registered_names.is_empty());
    assert_eq!(runtime.registered_names(), Vec::<String>::new());
    assert_eq!(runtime.resolve_alias(target_alias), None);
    assert_eq!(runtime.resolve_alias(neighbor_alias), Some(neighbor));
    assert_eq!(runtime.alias_count(), 1);
    assert_eq!(runtime.delayed_send_count(), 0);
    assert_eq!(
        runtime.read_delayed_send(target_timer, 0),
        Err(format!("missing timer {}", target_timer.as_u64()))
    );
    assert_eq!(
        runtime
            .memory_metrics(target)
            .expect("target memory metrics")
            .current_bytes,
        0
    );
    assert!(runtime
        .failure_snapshot(neighbor)
        .expect("neighbor relationships")
        .links
        .is_empty());
    assert_eq!(
        runtime
            .processes()
            .snapshot(neighbor)
            .expect("neighbor")
            .state,
        VmProcessState::Runnable
    );
    assert!(runtime.advance_actor_timers(50).deliveries.is_empty());
    assert_down_message(
        &mut runtime,
        observer,
        monitor_ref.as_u64(),
        target,
        ReplValue::Atom("normal".to_string()),
    );
}

#[test]
fn abnormal_link_cascade_cleans_every_exited_owner_without_cross_owner_damage() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("owner"));
    let linked = runtime.spawn_root(source("linked"));
    let observer = runtime.spawn_root(source("observer"));
    let survivor = runtime.spawn_root(source("survivor"));
    runtime.link_actors(owner, linked).expect("link cascade");
    let monitor_ref = runtime
        .monitor_actor(observer, linked)
        .expect("monitor linked actor");
    runtime
        .receive_next_or_block(observer)
        .expect("block observer");
    runtime.register_name("owner", owner).expect("owner name");
    runtime
        .register_name("linked", linked)
        .expect("linked name");
    runtime
        .register_name("survivor", survivor)
        .expect("survivor name");
    let owner_alias = runtime.create_alias(owner).expect("owner alias");
    let linked_alias = runtime.create_alias(linked).expect("linked alias");
    let survivor_alias = runtime.create_alias(survivor).expect("survivor alias");
    runtime
        .processes
        .get_mut(owner)
        .expect("owner")
        .add_resource_handle("owner:resource");
    runtime
        .processes
        .get_mut(linked)
        .expect("linked")
        .add_resource_handle("linked:resource");
    runtime
        .send(survivor, owner, ReplValue::Int(1))
        .expect("owner mailbox message");
    runtime
        .send(survivor, linked, ReplValue::Int(2))
        .expect("linked mailbox message");
    let owner_timer = runtime
        .send_after(owner, survivor, ReplValue::Int(10), 0, 50)
        .expect("owner timer");
    let linked_timer = runtime
        .send_after(linked, survivor, ReplValue::Int(20), 0, 50)
        .expect("linked timer");
    let survivor_timer = runtime
        .send_after(survivor, survivor, ReplValue::Int(30), 0, 50)
        .expect("survivor timer");

    let cleanup = runtime
        .exit_actor(owner, VmExitReason::Killed)
        .expect("cascade exit");

    assert_eq!(
        cleanup,
        vec!["owner:resource".to_string(), "linked:resource".to_string()]
    );
    for exited in [owner, linked] {
        let snapshot = runtime
            .processes()
            .snapshot(exited)
            .expect("exited snapshot");
        assert_eq!(snapshot.state, VmProcessState::Exited(VmExitReason::Killed));
        assert_eq!(snapshot.mailbox_messages, 0);
        assert_eq!(snapshot.heap_bytes, 0);
        assert!(snapshot.resource_handles.is_empty());
        assert!(snapshot.registered_names.is_empty());
        assert_eq!(
            runtime
                .memory_metrics(exited)
                .expect("exited memory metrics")
                .current_bytes,
            0
        );
    }
    assert_eq!(runtime.lookup_name("owner"), None);
    assert_eq!(runtime.lookup_name("linked"), None);
    assert_eq!(runtime.lookup_name("survivor"), Some(survivor));
    assert_eq!(runtime.resolve_alias(owner_alias), None);
    assert_eq!(runtime.resolve_alias(linked_alias), None);
    assert_eq!(runtime.resolve_alias(survivor_alias), Some(survivor));
    assert_eq!(runtime.alias_count(), 1);
    assert_eq!(runtime.delayed_send_count(), 1);
    for timer in [owner_timer, linked_timer] {
        assert_eq!(
            runtime.read_delayed_send(timer, 0),
            Err(format!("missing timer {}", timer.as_u64()))
        );
    }
    assert_eq!(runtime.read_delayed_send(survivor_timer, 0), Ok(50));
    let advanced = runtime.advance_actor_timers(50);
    assert_eq!(advanced.deliveries.len(), 1);
    assert!(matches!(
        runtime
            .receive_next_or_block(survivor)
            .expect("survivor timer payload"),
        VmActorReceive::Message(message) if message.payload == ReplValue::Int(30)
    ));
    assert_down_message(
        &mut runtime,
        observer,
        monitor_ref.as_u64(),
        linked,
        ReplValue::Atom("killed".to_string()),
    );
}

#[test]
fn suspended_monitor_observer_waits_for_explicit_resume_after_target_exit() {
    let mut runtime = VmActorRuntime::default();
    let observer = runtime.spawn_root(source("observer"));
    let target = runtime.spawn_root(source("target"));
    let monitor_ref = runtime
        .monitor_actor(observer, target)
        .expect("monitor target");
    runtime
        .receive_next_or_block(observer)
        .expect("block observer");
    runtime.suspend(observer).expect("suspend observer");

    runtime
        .exit_actor(target, VmExitReason::Killed)
        .expect("exit target");

    assert_eq!(
        runtime
            .processes()
            .snapshot(observer)
            .expect("observer")
            .state,
        VmProcessState::Suspended(VmProcessResumeState::Runnable)
    );
    let idle = runtime
        .run_next(|_, _| panic!("suspended observer must not execute"))
        .expect("discard stale target queue entry");
    assert_eq!(idle.outcome, VmSchedulerOutcome::Idle);

    runtime.resume(observer).expect("resume observer");
    let run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, observer);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("run resumed observer");
    assert_eq!(run.pid, Some(observer));
    assert_eq!(run.outcome, VmSchedulerOutcome::Blocked);
    assert_down_message(
        &mut runtime,
        observer,
        monitor_ref.as_u64(),
        target,
        ReplValue::Atom("killed".to_string()),
    );
}

#[test]
fn selective_receive_preserves_timer_noise_until_matching_payload_arrives() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime.spawn_root(source("recipient"));
    assert_eq!(
        runtime
            .selective_receive_or_block(recipient, |message| {
                message.payload == ReplValue::Atom("match".to_string())
            })
            .expect("initial selective receive"),
        VmActorReceive::Blocked
    );
    runtime
        .send_after(
            sender,
            recipient,
            ReplValue::Atom("noise".to_string()),
            0,
            10,
        )
        .expect("noise timer");
    runtime
        .send_after(
            sender,
            recipient,
            ReplValue::Atom("match".to_string()),
            0,
            20,
        )
        .expect("matching timer");

    assert_eq!(runtime.advance_actor_timers(10).deliveries.len(), 1);
    assert_eq!(
        runtime
            .selective_receive_or_block(recipient, |message| {
                message.payload == ReplValue::Atom("match".to_string())
            })
            .expect("noise must not satisfy receive"),
        VmActorReceive::Blocked
    );
    assert_eq!(
        runtime
            .processes()
            .snapshot(recipient)
            .expect("recipient")
            .mailbox_messages,
        1
    );

    assert_eq!(runtime.advance_actor_timers(20).deliveries.len(), 1);
    assert!(matches!(
        runtime
            .selective_receive_or_block(recipient, |message| {
                message.payload == ReplValue::Atom("match".to_string())
            })
            .expect("receive matching payload"),
        VmActorReceive::Message(message)
            if message.payload == ReplValue::Atom("match".to_string())
    ));
    assert!(matches!(
        runtime
            .receive_next_or_block(recipient)
            .expect("receive preserved noise"),
        VmActorReceive::Message(message)
            if message.payload == ReplValue::Atom("noise".to_string())
    ));
}
