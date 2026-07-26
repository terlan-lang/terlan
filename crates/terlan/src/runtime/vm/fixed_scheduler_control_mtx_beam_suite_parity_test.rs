//! VM-owned replacements for the portable outcomes of OTP `mtx_SUITE`.

use std::collections::BTreeSet;
use std::num::NonZeroU64;
use std::sync::{Arc, Barrier};
use std::thread;

use super::*;
use crate::runtime::vm::actor_directory::ACTOR_MAILBOX_CAPACITY;
use crate::runtime::vm::scheduler_topology::VmSchedulerTopology;

const PRODUCERS: usize = 20;
const WRITERS: usize = 6;
const COMMANDS_PER_PRODUCER: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OwnerCommand {
    Write { producer: usize, ordinal: usize },
    Read { producer: usize, ordinal: usize },
}

fn route() -> VmFixedActorRoute {
    VmSchedulerTopology::new(4)
        .expect("four-scheduler topology")
        .route(NonZeroU64::new(1).expect("nonzero actor"))
}

fn register_parked<P>(control: &VmFixedSchedulerControl<P>) -> VmFixedActorRoute {
    let route = route();
    control.register(route).expect("register owner actor");
    let lease = control
        .acquire(route, route.scheduler())
        .expect("acquire owner actor");
    assert_eq!(
        control
            .release(lease, VmActorLifecycle::Parked)
            .expect("park owner actor"),
        VmActorLifecycle::Parked
    );
    route
}

#[test]
fn concurrent_read_write_pressure_is_serialized_by_one_actor_owner() {
    let control = Arc::new(VmFixedSchedulerControl::default());
    let route = register_parked(&control);
    let start = Arc::new(Barrier::new(PRODUCERS + 1));
    let producers = (0..PRODUCERS)
        .map(|producer| {
            let control = Arc::clone(&control);
            let start = Arc::clone(&start);
            thread::spawn(move || {
                start.wait();
                for ordinal in 0..COMMANDS_PER_PRODUCER {
                    let command = if producer < WRITERS {
                        OwnerCommand::Write { producer, ordinal }
                    } else {
                        OwnerCommand::Read { producer, ordinal }
                    };
                    control
                        .publish_identified(route, command)
                        .expect("bounded publication");
                }
            })
        })
        .collect::<Vec<_>>();
    start.wait();
    for producer in producers {
        producer.join().expect("producer remains healthy");
    }

    let lease = control
        .acquire(route, route.scheduler())
        .expect("single owner drains commands");
    let commands = control
        .drain_identified(&lease)
        .expect("owner drains complete commands");
    assert_eq!(commands.len(), PRODUCERS * COMMANDS_PER_PRODUCER);
    assert_eq!(
        commands
            .iter()
            .map(|(publication, _)| publication.sequence)
            .collect::<BTreeSet<_>>(),
        (1..=commands.len() as u64).collect()
    );

    let mut per_producer = vec![Vec::new(); PRODUCERS];
    let mut owner_value = 0usize;
    let mut read_observations = Vec::new();
    for (_, command) in commands {
        match command {
            OwnerCommand::Write { producer, ordinal } => {
                per_producer[producer].push(ordinal);
                owner_value += 1;
            }
            OwnerCommand::Read { producer, ordinal } => {
                per_producer[producer].push(ordinal);
                read_observations.push(owner_value);
            }
        }
    }
    for ordinals in per_producer {
        assert_eq!(
            ordinals,
            (0..COMMANDS_PER_PRODUCER).collect::<Vec<_>>(),
            "one producer's publication order changed"
        );
    }
    assert_eq!(owner_value, WRITERS * COMMANDS_PER_PRODUCER);
    assert_eq!(
        read_observations.len(),
        (PRODUCERS - WRITERS) * COMMANDS_PER_PRODUCER
    );
    assert!(read_observations.iter().all(|value| *value <= owner_value));

    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("release terminal owner");
    control.reclaim(route).expect("reclaim owner actor");
}

#[test]
fn bounded_try_publication_rejects_pressure_and_recovers_exact_capacity() {
    let control = VmFixedSchedulerControl::default();
    let route = register_parked(&control);
    for value in 0..ACTOR_MAILBOX_CAPACITY {
        control
            .publish_identified(route, value)
            .expect("fill exact bounded capacity");
    }
    assert!(
        control
            .publish_identified(route, ACTOR_MAILBOX_CAPACITY)
            .expect_err("over-capacity publication must fail immediately")
            .contains("MailboxFull"),
        "pressure rejection must retain its typed cause"
    );

    let lease = control
        .acquire(route, route.scheduler())
        .expect("owner drains full mailbox");
    let first = control
        .drain_identified(&lease)
        .expect("drain full mailbox");
    assert_eq!(first.len(), ACTOR_MAILBOX_CAPACITY);
    assert_eq!(first.first().map(|row| row.0.sequence), Some(1));
    assert_eq!(
        first.last().map(|row| row.0.sequence),
        Some(ACTOR_MAILBOX_CAPACITY as u64)
    );
    control
        .release(lease, VmActorLifecycle::Parked)
        .expect("park after drain");

    let (publication, _) = control
        .publish_identified(route, ACTOR_MAILBOX_CAPACITY)
        .expect("capacity recovers after owner drain");
    assert_eq!(publication.sequence, ACTOR_MAILBOX_CAPACITY as u64 + 1);
    let lease = control
        .acquire(route, route.scheduler())
        .expect("owner reacquires after recovery");
    assert_eq!(
        control.drain(&lease).expect("drain recovered command"),
        vec![ACTOR_MAILBOX_CAPACITY]
    );
    control
        .release(lease, VmActorLifecycle::Exiting)
        .expect("release terminal owner");
    control.reclaim(route).expect("reclaim owner actor");
}

#[test]
fn held_mutator_rejects_every_contender_then_advances_owner_generation() {
    let control = Arc::new(VmFixedSchedulerControl::<()>::default());
    let route = route();
    control.register(route).expect("register owner actor");
    let lease = control
        .acquire(route, route.scheduler())
        .expect("first owner acquisition");
    let first_generation = lease.owner_generation();
    let start = Arc::new(Barrier::new(PRODUCERS));
    let contenders = (1..PRODUCERS)
        .map(|_| {
            let control = Arc::clone(&control);
            let start = Arc::clone(&start);
            thread::spawn(move || {
                start.wait();
                control.acquire(route, route.scheduler()).is_err()
            })
        })
        .collect::<Vec<_>>();
    start.wait();
    assert!(
        contenders
            .into_iter()
            .all(|contender| contender.join().expect("contender remains healthy")),
        "a second scheduler acquired mutable actor state"
    );

    control
        .release(lease, VmActorLifecycle::Yielding)
        .expect("release first owner");
    control
        .requeue_yielded(route)
        .expect("requeue released actor");
    let next = control
        .acquire(route, route.scheduler())
        .expect("next owner acquisition");
    assert!(next.owner_generation() > first_generation);
    control
        .release(next, VmActorLifecycle::Exiting)
        .expect("release terminal owner");
    control.reclaim(route).expect("reclaim owner actor");
}
