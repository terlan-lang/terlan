//! Portable contracts mined from OTP's `parallel_messages_SUITE`.

use std::sync::{Arc, Barrier};
use std::thread;

use super::{VmActorDirectory, VmActorDirectoryError};
use crate::runtime::vm::process::VmProcessId;

const PRODUCERS: u64 = 4;
const EVENTS_PER_PRODUCER: u64 = 160;

#[derive(Clone, Debug, Eq, PartialEq)]
enum ParallelEvent {
    Message {
        sender: u64,
        ordinal: u64,
        words: Vec<u64>,
    },
    TrappedExit {
        sender: u64,
        ordinal: u64,
        reason_words: Vec<u64>,
    },
}

impl ParallelEvent {
    fn sender_and_ordinal(&self) -> (u64, u64) {
        match self {
            Self::Message {
                sender, ordinal, ..
            }
            | Self::TrappedExit {
                sender, ordinal, ..
            } => (*sender, *ordinal),
        }
    }

    fn payload(&self) -> &[u64] {
        match self {
            Self::Message { words, .. } => words,
            Self::TrappedExit { reason_words, .. } => reason_words,
        }
    }
}

fn receiver() -> VmProcessId {
    VmProcessId::from_raw_for_test(1)
}

fn event(sender: u64, ordinal: u64) -> ParallelEvent {
    let payload_words = match ordinal % 4 {
        0 => 1,
        1 => 10,
        2 => 100,
        _ => 1_000,
    };
    let payload = (0..payload_words)
        .map(|index| sender * 10_000 + ordinal * 1_000 + index)
        .collect::<Vec<_>>();
    if ordinal % 2 == 0 {
        ParallelEvent::Message {
            sender,
            ordinal,
            words: payload,
        }
    } else {
        ParallelEvent::TrappedExit {
            sender,
            ordinal,
            reason_words: payload,
        }
    }
}

#[test]
fn parallel_messages_suite_mixed_multi_sender_delivery_is_complete_and_ordered() {
    let actor = receiver();
    let mut directory: VmActorDirectory<(), ParallelEvent> = VmActorDirectory::default();
    directory.insert(actor, ()).expect("insert receiver");
    let directory = Arc::new(directory);
    let start = Arc::new(Barrier::new(PRODUCERS as usize + 1));
    let producers = (0..PRODUCERS)
        .map(|sender| {
            let directory = Arc::clone(&directory);
            let start = Arc::clone(&start);
            thread::spawn(move || {
                start.wait();
                for ordinal in 0..EVENTS_PER_PRODUCER {
                    directory
                        .publish_fragment(actor, event(sender, ordinal))
                        .expect("mixed message and trapped-exit publication");
                    if (sender + ordinal) % 7 == 0 {
                        thread::yield_now();
                    }
                }
            })
        })
        .collect::<Vec<_>>();
    start.wait();
    for producer in producers {
        producer.join().expect("producer remains healthy");
    }

    assert_eq!(
        directory
            .pending_publications(actor)
            .expect("inspect complete publications"),
        (PRODUCERS * EVENTS_PER_PRODUCER) as usize
    );
    let mut directory = Arc::try_unwrap(directory).expect("all producers released directory");
    let token = directory
        .acquire_control_mutator(actor, 1)
        .expect("single receiver owns integration");
    let mut sequences = Vec::new();
    let mut by_sender = vec![Vec::new(); PRODUCERS as usize];
    let drained = directory
        .drain_publications(&token, |_state, publication, payload| {
            let (sender, ordinal) = payload.sender_and_ordinal();
            let expected = event(sender, ordinal);
            assert_eq!(payload, expected);
            assert!(!payload.payload().is_empty());
            sequences.push(publication.sequence);
            by_sender[sender as usize].push(ordinal);
        })
        .expect("integrate every mixed publication");
    directory
        .release_control_mutator(token)
        .expect("release receiver ownership");

    assert_eq!(drained, (PRODUCERS * EVENTS_PER_PRODUCER) as usize);
    for ordinals in by_sender {
        assert_eq!(ordinals, (0..EVENTS_PER_PRODUCER).collect::<Vec<_>>());
    }
    sequences.sort_unstable();
    assert_eq!(
        sequences,
        (1..=PRODUCERS * EVENTS_PER_PRODUCER).collect::<Vec<_>>()
    );
    assert_eq!(
        directory
            .pending_publications(actor)
            .expect("mailbox is empty after complete drain"),
        0
    );
}

#[test]
fn parallel_messages_suite_bounded_pressure_recovers_without_retained_fragments() {
    const CAPACITY: usize = 1_024;

    let actor = receiver();
    let mut directory: VmActorDirectory<(), u64> = VmActorDirectory::default();
    let handle = directory.insert(actor, ()).expect("insert receiver");
    for value in 0..CAPACITY as u64 {
        directory
            .publish_fragment(actor, value)
            .expect("fill exact bounded mailbox");
    }
    assert_eq!(
        directory.publish_fragment(actor, CAPACITY as u64),
        Err(VmActorDirectoryError::MailboxFull(handle))
    );

    for cycle in 0..2 {
        let token = directory
            .acquire_control_mutator(actor, 1)
            .expect("single receiver owns drain");
        let drained = directory
            .drain_publications(&token, |_state, publication, value| {
                assert_eq!(publication.sequence, cycle * CAPACITY as u64 + value + 1);
            })
            .expect("drain admitted fragments");
        directory
            .release_control_mutator(token)
            .expect("release receiver ownership");
        assert_eq!(drained, CAPACITY);
        assert_eq!(
            directory
                .pending_publications(actor)
                .expect("mailbox is empty after drain"),
            0
        );
        if cycle == 0 {
            for value in 0..CAPACITY as u64 {
                directory
                    .publish_fragment(actor, value)
                    .expect("all credits return after drain");
            }
        }
    }
}
