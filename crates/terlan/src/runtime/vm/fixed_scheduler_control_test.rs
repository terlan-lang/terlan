use std::num::NonZeroU64;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use super::*;
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;

fn route(topology: &VmSchedulerTopology, actor: u64) -> VmFixedActorRoute {
    topology.route(NonZeroU64::new(actor).expect("actor identity"))
}

#[test]
fn remote_publication_wakes_home_scheduler_and_preserves_payload_order() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let actor = route(&topology, 2);
    let control = Arc::new(VmFixedSchedulerControl::default());
    control.register(actor).expect("register actor");
    let lease = control
        .acquire(actor, actor.scheduler())
        .expect("initial execution");
    control
        .release(lease, VmActorLifecycle::Parked)
        .expect("park actor");

    let producer = Arc::clone(&control);
    thread::spawn(move || {
        assert_eq!(
            producer.publish(actor, 10).expect("first publish"),
            VmMailboxWake::Enqueue
        );
        assert_eq!(
            producer.publish(actor, 20).expect("second publish"),
            VmMailboxWake::Observed
        );
    })
    .join()
    .expect("producer thread");

    assert_eq!(
        control.lifecycle(actor).expect("queued lifecycle"),
        VmActorLifecycle::Queued
    );
    let lease = control
        .acquire(actor, actor.scheduler())
        .expect("home scheduler acquires wake");
    assert_eq!(control.drain(&lease).expect("drain payloads"), vec![10, 20]);
    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("terminal release");
    control.reclaim(actor).expect("reclaim actor");
}

#[test]
fn two_scheduler_leases_overlap_without_global_execution_lock() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let first = route(&topology, 1);
    let second = route(&topology, 2);
    let control = Arc::new(VmFixedSchedulerControl::<()>::default());
    control.register(first).expect("first actor");
    control.register(second).expect("second actor");
    let barrier = Arc::new(Barrier::new(2));
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));

    let workers = [first, second].map(|actor| {
        let control = Arc::clone(&control);
        let barrier = Arc::clone(&barrier);
        let active = Arc::clone(&active);
        let maximum = Arc::clone(&maximum);
        thread::spawn(move || {
            let lease = control
                .acquire(actor, actor.scheduler())
                .expect("fixed scheduler acquisition");
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(now, Ordering::SeqCst);
            barrier.wait();
            active.fetch_sub(1, Ordering::SeqCst);
            control
                .release(lease, VmActorLifecycle::Exiting)
                .expect("terminal release");
            control.reclaim(actor).expect("reclaim actor");
        })
    });
    for worker in workers {
        worker.join().expect("scheduler thread");
    }
    assert_eq!(maximum.load(Ordering::SeqCst), 2);
}

#[test]
fn wrong_scheduler_and_duplicate_registration_are_side_effect_free() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let actor = route(&topology, 1);
    let control = VmFixedSchedulerControl::<()>::default();
    control.register(actor).expect("register actor");
    assert!(control
        .register(actor)
        .expect_err("duplicate")
        .contains("register"));
    let wrong = topology.home_scheduler(NonZeroU64::new(2).expect("peer"));
    assert!(control
        .acquire(actor, wrong)
        .expect_err("wrong scheduler")
        .contains("belongs to scheduler"));
    assert_eq!(
        control.lifecycle(actor).expect("still queued"),
        VmActorLifecycle::Queued
    );
}

#[test]
fn one_scheduler_transition_order_remains_stable() {
    let topology = VmSchedulerTopology::new(1).expect("topology");
    let actor = route(&topology, 1);
    let control = VmFixedSchedulerControl::<()>::default();
    control.register(actor).expect("register actor");
    let lease = control.acquire(actor, actor.scheduler()).expect("acquire");
    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("release");
    control.reclaim(actor).expect("reclaim");
    let transitions = control.transition_events().expect("transitions");
    assert_eq!(
        transitions
            .iter()
            .map(|event| (event.from, event.to, event.owner))
            .collect::<Vec<_>>(),
        vec![
            (VmActorLifecycle::Yielding, VmActorLifecycle::Queued, 0),
            (VmActorLifecycle::Queued, VmActorLifecycle::Executing, 1),
            (VmActorLifecycle::Executing, VmActorLifecycle::Exiting, 1),
            (VmActorLifecycle::Exiting, VmActorLifecycle::Retired, 0),
            (VmActorLifecycle::Retired, VmActorLifecycle::Reclaimed, 0),
        ]
    );
}

#[test]
fn yielded_actor_must_be_requeued_before_its_next_execution_lease() {
    let topology = VmSchedulerTopology::new(1).expect("topology");
    let actor = route(&topology, 1);
    let control = VmFixedSchedulerControl::<()>::default();
    control.register(actor).expect("register actor");
    let lease = control.acquire(actor, actor.scheduler()).expect("acquire");
    control
        .release(lease, VmActorLifecycle::Yielding)
        .expect("yield actor");
    assert!(control
        .acquire(actor, actor.scheduler())
        .expect_err("yielding actor is not queued")
        .contains("acquire"));
    control.requeue_yielded(actor).expect("requeue actor");
    let lease = control
        .acquire(actor, actor.scheduler())
        .expect("reacquire queued actor");
    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("exit actor");
    control.reclaim(actor).expect("reclaim actor");
}

#[test]
fn queued_actor_migration_publishes_destination_before_reacquisition() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let source = route(&topology, 1);
    let destination_scheduler = route(&topology, 2).scheduler();
    let control = VmFixedSchedulerControl::<()>::default();
    control.register(source).expect("register queued actor");
    let ticket = control
        .begin_migration(source, destination_scheduler)
        .expect("claim queued actor");
    assert_eq!(
        control.lifecycle(source).expect("migrating actor"),
        VmActorLifecycle::Migrating
    );
    let destination = control
        .complete_migration(ticket)
        .expect("publish destination route");
    assert_eq!(destination.home_scheduler(), source.home_scheduler());
    assert_eq!(destination.scheduler(), destination_scheduler);
    assert_eq!(
        control.lifecycle(destination).expect("queued destination"),
        VmActorLifecycle::Queued
    );
    let lease = control
        .acquire(destination, destination_scheduler)
        .expect("destination acquisition");
    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("exit actor");
    control.reclaim(destination).expect("reclaim actor");
}

#[test]
fn shutdown_reclaims_queued_and_parked_actors() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let queued = route(&topology, 1);
    let parked = route(&topology, 2);
    let control = VmFixedSchedulerControl::<()>::default();
    control.register(queued).expect("queued actor");
    control.register(parked).expect("parked actor");
    let lease = control
        .acquire(parked, parked.scheduler())
        .expect("acquire parked actor");
    control
        .release(lease, VmActorLifecycle::Parked)
        .expect("park actor");

    assert_eq!(control.shutdown().expect("shutdown control"), 2);
    assert!(control.lifecycle(queued).is_err());
    assert!(control.lifecycle(parked).is_err());
    assert_eq!(control.shutdown().expect("idempotent empty shutdown"), 0);
}

#[test]
fn shutdown_rejects_executing_actor_without_partial_reclaim() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let idle = route(&topology, 1);
    let executing = route(&topology, 2);
    let control = VmFixedSchedulerControl::<()>::default();
    control.register(idle).expect("idle actor");
    control.register(executing).expect("executing actor");
    let lease = control
        .acquire(executing, executing.scheduler())
        .expect("execution lease");

    let error = control
        .shutdown()
        .expect_err("executing actor blocks shutdown");
    assert!(error.contains("still executing"), "{error}");
    assert_eq!(
        control.lifecycle(idle).expect("idle actor retained"),
        VmActorLifecycle::Queued
    );
    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("terminal release");
    control.reclaim(executing).expect("reclaim execution");
    assert_eq!(control.shutdown().expect("finish shutdown"), 1);
}

#[test]
fn concurrent_publication_storm_has_one_wake_and_complete_delivery() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let actor = route(&topology, 2);
    let control = Arc::new(VmFixedSchedulerControl::default());
    control.register(actor).expect("register actor");
    let lease = control.acquire(actor, actor.scheduler()).expect("acquire");
    control
        .release(lease, VmActorLifecycle::Parked)
        .expect("park actor");
    let barrier = Arc::new(Barrier::new(9));
    let wakes = Arc::new(AtomicUsize::new(0));
    let producers = (0..8)
        .map(|producer| {
            let control = Arc::clone(&control);
            let barrier = Arc::clone(&barrier);
            let wakes = Arc::clone(&wakes);
            thread::spawn(move || {
                barrier.wait();
                for item in 0..8 {
                    if control
                        .publish(actor, producer * 8 + item)
                        .expect("publish")
                        == VmMailboxWake::Enqueue
                    {
                        wakes.fetch_add(1, Ordering::SeqCst);
                    }
                }
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    for producer in producers {
        producer.join().expect("producer");
    }

    assert_eq!(wakes.load(Ordering::SeqCst), 1);
    let lease = control
        .acquire(actor, actor.scheduler())
        .expect("wake lease");
    let mut payloads = control.drain(&lease).expect("drain storm");
    payloads.sort_unstable();
    assert_eq!(payloads, (0..64).collect::<Vec<_>>());
    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("terminal release");
    control.reclaim(actor).expect("reclaim actor");
}

#[test]
fn simultaneous_actor_registration_preserves_unique_fixed_placement() {
    let topology = VmSchedulerTopology::new(4).expect("topology");
    let control = Arc::new(VmFixedSchedulerControl::<()>::default());
    let barrier = Arc::new(Barrier::new(33));
    let workers = (1..=32)
        .map(|actor| {
            let route = route(&topology, actor);
            let control = Arc::clone(&control);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                control.register(route).expect("concurrent registration");
                route
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    let routes = workers
        .into_iter()
        .map(|worker| worker.join().expect("registration worker"))
        .collect::<Vec<_>>();
    for actor in routes {
        assert_eq!(
            control.lifecycle(actor).expect("registered lifecycle"),
            VmActorLifecycle::Queued
        );
    }
    assert_eq!(control.shutdown().expect("reclaim registrations"), 32);
}

#[test]
fn actor_repeatedly_migrates_between_explicit_schedulers_without_aba_reuse() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let mut current = route(&topology, 1);
    let home = current.home_scheduler();
    let peer = topology.schedulers().nth(1).expect("peer scheduler");
    let control = VmFixedSchedulerControl::<usize>::default();
    control.register(current).expect("register actor");
    let lease = control
        .acquire(current, current.scheduler())
        .expect("acquire");
    control
        .release(lease, VmActorLifecycle::Parked)
        .expect("park actor");

    for sequence in 0..100 {
        let destination = if current.scheduler() == home {
            peer
        } else {
            home
        };
        let ticket = control
            .begin_migration(current, destination)
            .expect("begin migration");
        assert_eq!(ticket.source(), current);
        assert_eq!(ticket.destination().scheduler(), destination);
        let stale = ticket.duplicate_for_test();
        assert_eq!(
            control
                .publish(current, sequence)
                .expect("publish in transfer"),
            VmMailboxWake::Enqueue
        );
        current = control
            .complete_migration(ticket)
            .expect("complete migration");
        assert_eq!(current.home_scheduler(), home);
        let error = control
            .complete_migration(stale)
            .expect_err("consumed ticket must remain stale");
        assert!(error.contains("migration_stale"), "{error}");
        let lease = control
            .acquire(current, destination)
            .expect("destination acquires actor");
        assert_eq!(
            control.drain(&lease).expect("drain payload"),
            vec![sequence]
        );
        control
            .release(lease, VmActorLifecycle::Parked)
            .expect("park after destination execution");
    }
    assert_eq!(
        control.publish(current, 100).expect("terminal wake"),
        VmMailboxWake::Enqueue
    );
    let lease = control
        .acquire(current, current.scheduler())
        .expect("terminal acquire");
    assert_eq!(
        control.drain(&lease).expect("drain terminal wake"),
        vec![100]
    );
    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("terminal release");
    control.reclaim(current).expect("reclaim actor");
}

#[test]
fn migration_abort_restores_source_and_executing_actor_cannot_transfer() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let source = route(&topology, 1);
    let destination = topology.schedulers().nth(1).expect("destination");
    let control = VmFixedSchedulerControl::<()>::default();
    control.register(source).expect("register actor");
    let lease = control
        .acquire(source, source.scheduler())
        .expect("acquire");
    let error = control
        .begin_migration(source, destination)
        .expect_err("owned actor cannot migrate");
    assert!(error.contains("begin migration"), "{error}");
    control
        .release(lease, VmActorLifecycle::Parked)
        .expect("park actor");
    let ticket = control
        .begin_migration(source, destination)
        .expect("begin transfer");
    assert_eq!(
        control.abort_migration(ticket).expect("abort transfer"),
        source
    );
    assert_eq!(
        control.lifecycle(source).expect("restored lifecycle"),
        VmActorLifecycle::Parked
    );
    assert_eq!(
        control.publish(source, ()).expect("wake restored source"),
        VmMailboxWake::Enqueue
    );
    let lease = control
        .acquire(source, source.scheduler())
        .expect("source reacquires actor");
    assert_eq!(control.drain(&lease).expect("drain wake"), vec![()]);
    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("terminal release");
    control.reclaim(source).expect("reclaim actor");
}
