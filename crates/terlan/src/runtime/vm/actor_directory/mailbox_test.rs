use std::sync::{Arc, Barrier};
use std::thread;

use super::{
    VmActorMailbox, VmMailboxWake, ACTIVE, ACTOR_MAILBOX_CAPACITY, NOTIFIED, PARKED, PARKING,
};
use crate::runtime::vm::actor_directory::VmActorDirectoryError;
use crate::runtime::vm::actor_directory::VmActorHandle;
use crate::runtime::vm::process::VmProcessId;

#[derive(Clone, Copy)]
enum ModelStep {
    ReceiverPrepare,
    ReceiverCheck,
    ReceiverPark,
    ReceiverRelease,
    ReceiverRecheck,
    ProducerPush,
    ProducerNotify,
    ProducerPromote,
}

#[derive(Clone, Copy, Default)]
struct ModelState {
    queue_visible: bool,
    wake_state: u8,
    lifecycle: u8,
    producer_observed_parked: bool,
}

fn handle() -> VmActorHandle {
    VmActorHandle {
        pid: VmProcessId::from_raw_for_test(1),
        slot: 0,
        actor_generation: 1,
    }
}

#[test]
fn mpsc_publication_preserves_each_sender_order_and_unique_sequences() {
    let mailbox = Arc::new(VmActorMailbox::default());
    let start = Arc::new(Barrier::new(5));
    let producers = (0_u64..4)
        .map(|sender| {
            let mailbox = Arc::clone(&mailbox);
            let start = Arc::clone(&start);
            thread::spawn(move || {
                start.wait();
                for ordinal in 0_u64..128 {
                    mailbox
                        .publish(handle(), (sender, ordinal))
                        .expect("bounded fixture remains below capacity");
                }
            })
        })
        .collect::<Vec<_>>();
    start.wait();
    for producer in producers {
        producer.join().expect("producer remains healthy");
    }

    let mut observed = vec![Vec::new(), Vec::new(), Vec::new(), Vec::new()];
    let mut sequences = Vec::new();
    assert_eq!(
        mailbox.drain(|fragment| {
            sequences.push(fragment.publication.sequence);
            observed[fragment.payload.0 as usize].push(fragment.payload.1);
        }),
        512
    );
    for sender in observed {
        assert_eq!(sender, (0_u64..128).collect::<Vec<_>>());
    }
    sequences.sort_unstable();
    assert_eq!(sequences, (1_u64..=512).collect::<Vec<_>>());
}

#[test]
fn seeded_mailbox_flood_preserves_every_sender_under_forced_interleaving() {
    let mailbox = Arc::new(VmActorMailbox::default());
    let producers = (0_u64..8)
        .map(|sender| {
            let mailbox = Arc::clone(&mailbox);
            thread::spawn(move || {
                let mut seed = 0x9e37_79b9_7f4a_7c15_u64 ^ sender;
                for ordinal in 0_u64..128 {
                    seed = seed
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    if seed & 3 == 0 {
                        thread::yield_now();
                    }
                    mailbox
                        .publish(handle(), (sender, ordinal))
                        .expect("seeded flood fits exact bounded capacity");
                }
            })
        })
        .collect::<Vec<_>>();
    for producer in producers {
        producer.join().expect("producer remains healthy");
    }

    let mut observed = vec![Vec::new(); 8];
    assert_eq!(
        mailbox.drain(|fragment| {
            observed[fragment.payload.0 as usize].push(fragment.payload.1);
        }),
        ACTOR_MAILBOX_CAPACITY
    );
    for sender in observed {
        assert_eq!(sender, (0_u64..128).collect::<Vec<_>>());
    }
}

#[test]
fn parked_receiver_gets_exact_wake_action_and_active_receiver_does_not() {
    let mailbox = VmActorMailbox::default();
    assert!(mailbox.prepare_park());
    let (_, wake) = mailbox.publish(handle(), 1_u8).expect("first publish");
    assert_eq!(wake, VmMailboxWake::Enqueue);
    let (_, duplicate) = mailbox.publish(handle(), 2_u8).expect("second publish");
    assert_eq!(duplicate, VmMailboxWake::Observed);
    assert_eq!(mailbox.drain(|_| {}), 2);
    mailbox.activate();
    let (_, active) = mailbox.publish(handle(), 3_u8).expect("active publish");
    assert_eq!(active, VmMailboxWake::Observed);
}

#[test]
fn pending_publication_prevents_receiver_from_parking() {
    let mailbox = VmActorMailbox::default();
    mailbox.publish(handle(), 1_u8).expect("publish");
    assert!(!mailbox.prepare_park());
    assert_eq!(mailbox.len(), 1);
    assert_eq!(mailbox.drain(|_| {}), 1);
    assert!(mailbox.prepare_park());
}

#[test]
fn repeated_park_publish_drain_cycles_never_lose_a_fragment() {
    let mailbox = Arc::new(VmActorMailbox::default());
    for value in 0_u64..1_000 {
        assert!(mailbox.prepare_park());
        let producer = Arc::clone(&mailbox);
        let join = thread::spawn(move || producer.publish(handle(), value));
        let (_, wake) = join
            .join()
            .expect("producer remains healthy")
            .expect("drained mailbox retains capacity");
        assert_eq!(wake, VmMailboxWake::Enqueue);
        let mut received = None;
        assert_eq!(
            mailbox.drain(|fragment| received = Some(fragment.payload)),
            1
        );
        assert_eq!(received, Some(value));
        mailbox.activate();
    }
}

#[test]
fn bounded_park_publish_interleavings_never_leave_message_only_parked() {
    let receiver = [
        ModelStep::ReceiverPrepare,
        ModelStep::ReceiverCheck,
        ModelStep::ReceiverPark,
        ModelStep::ReceiverRelease,
        ModelStep::ReceiverRecheck,
    ];
    let producer = [
        ModelStep::ProducerPush,
        ModelStep::ProducerNotify,
        ModelStep::ProducerPromote,
    ];
    let mut schedules = Vec::new();
    enumerate_schedules(&receiver, &producer, Vec::new(), &mut schedules);
    assert_eq!(schedules.len(), 56);

    for schedule in schedules {
        let mut state = ModelState::default();
        for step in schedule {
            apply_model_step(&mut state, step);
        }
        assert!(state.queue_visible);
        assert_ne!(
            state.lifecycle, PARKED,
            "a published fragment must leave the actor queued or yielding"
        );
    }
}

fn enumerate_schedules(
    receiver: &[ModelStep],
    producer: &[ModelStep],
    prefix: Vec<ModelStep>,
    schedules: &mut Vec<Vec<ModelStep>>,
) {
    if receiver.is_empty() && producer.is_empty() {
        schedules.push(prefix);
        return;
    }
    if let Some((step, remaining)) = receiver.split_first() {
        let mut next = prefix.clone();
        next.push(*step);
        enumerate_schedules(remaining, producer, next, schedules);
    }
    if let Some((step, remaining)) = producer.split_first() {
        let mut next = prefix;
        next.push(*step);
        enumerate_schedules(receiver, remaining, next, schedules);
    }
}

fn apply_model_step(state: &mut ModelState, step: ModelStep) {
    match step {
        ModelStep::ReceiverPrepare if state.wake_state == ACTIVE => {
            state.wake_state = PARKING;
        }
        ModelStep::ReceiverCheck if state.wake_state == PARKING && state.queue_visible => {
            state.wake_state = NOTIFIED;
        }
        ModelStep::ReceiverPark if state.wake_state == PARKING => {
            state.wake_state = PARKED;
        }
        ModelStep::ReceiverRelease => {
            state.lifecycle = if state.wake_state == PARKED {
                PARKED
            } else {
                ACTIVE
            };
        }
        ModelStep::ReceiverRecheck if state.lifecycle == PARKED && state.wake_state == NOTIFIED => {
            state.lifecycle = NOTIFIED;
        }
        ModelStep::ProducerPush => state.queue_visible = true,
        ModelStep::ProducerNotify => {
            state.producer_observed_parked = state.wake_state == PARKED;
            state.wake_state = NOTIFIED;
        }
        ModelStep::ProducerPromote
            if state.producer_observed_parked && state.lifecycle == PARKED =>
        {
            state.lifecycle = NOTIFIED;
        }
        _ => {}
    }
}

#[test]
fn bounded_mailbox_rejects_pressure_without_discarding_visible_fragments() {
    let mailbox = VmActorMailbox::default();
    for value in 0..ACTOR_MAILBOX_CAPACITY {
        mailbox
            .publish(handle(), value)
            .expect("mailbox admits its declared capacity");
    }
    assert!(matches!(
        mailbox.publish(handle(), ACTOR_MAILBOX_CAPACITY),
        Err(VmActorDirectoryError::MailboxFull(full)) if full == handle()
    ));
    let mut observed = Vec::new();
    assert_eq!(
        mailbox.drain(|fragment| observed.push(fragment.payload)),
        ACTOR_MAILBOX_CAPACITY
    );
    assert_eq!(observed, (0..ACTOR_MAILBOX_CAPACITY).collect::<Vec<_>>());
    let (publication, _) = mailbox
        .publish(handle(), ACTOR_MAILBOX_CAPACITY + 1)
        .expect("capacity returns after drain");
    assert_eq!(
        publication.sequence,
        ACTOR_MAILBOX_CAPACITY as u64 + 1,
        "rejected publication must not consume a sequence"
    );
}
