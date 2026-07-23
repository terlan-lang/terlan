use super::*;

#[test]
fn protocol_routes_are_stable_and_processes_are_unique() {
    let scheduler = VmSchedulerId::primary();
    let first = next_protocol_task_route(scheduler).expect("first protocol route");
    let second = next_protocol_task_route(scheduler).expect("second protocol route");

    assert_eq!(first.scheduler, scheduler);
    assert_eq!(second.scheduler, scheduler);
    assert_ne!(first.process, second.process);
}

#[test]
fn protocol_local_resources_keep_the_hot_identity_in_a_direct_slot() {
    let mut resources = VmProtocolLocalResources::default();
    let mut initializations = 0;
    let first = resources
        .with_resource(
            41,
            || {
                initializations += 1;
                Ok::<_, String>(7_u64)
            },
            |value| {
                *value += 1;
                Ok(*value)
            },
        )
        .expect("initialize active resource");
    let second = resources
        .with_resource(
            41,
            || {
                initializations += 1;
                Ok::<_, String>(0_u64)
            },
            |value| {
                *value += 1;
                Ok(*value)
            },
        )
        .expect("reuse active resource");

    assert_eq!((first, second), (8, 9));
    assert_eq!(initializations, 1);
    assert!(resources.inactive.is_empty());
}

#[test]
fn protocol_local_resources_demote_and_retire_cold_identities() {
    let mut resources = VmProtocolLocalResources::default();
    resources
        .with_resource(10, || Ok::<_, String>("ten".to_string()), |_| Ok(()))
        .expect("first resource");
    resources
        .with_resource(20, || Ok::<_, String>(20_u64), |_| Ok(()))
        .expect("second resource");
    assert_eq!(resources.inactive.len(), 1);

    resources
        .with_resource(
            10,
            || Err::<String, _>("must reuse cold resource".to_string()),
            |value| {
                assert_eq!(value, "ten");
                Ok(())
            },
        )
        .expect("promote cold resource");
    resources.retire(10);
    assert!(resources.active.is_none());
    assert_eq!(resources.inactive.len(), 1);
    resources.retire(20);
    assert!(resources.inactive.is_empty());
}

#[test]
fn generation_tagged_task_tokens_reuse_slots_without_aliasing_stale_events() {
    let slot = 7;
    let first = Token(FIRST_TASK_TOKEN + MAX_TASKS_PER_SHARD + slot);
    let second = Token(FIRST_TASK_TOKEN + (2 * MAX_TASKS_PER_SHARD) + slot);

    assert_eq!(task_slot_for_token(first), Some(slot));
    assert_eq!(task_slot_for_token(second), Some(slot));
    assert_ne!(first, second);
    assert_eq!(task_slot_for_token(ACCEPTOR_LISTENER_TOKEN), None);
    assert_eq!(task_slot_for_token(TASK_WAKE_TOKEN), None);
}

#[test]
fn readiness_wake_preserves_vm_ownership_and_transport_state() {
    let route = next_protocol_task_route(VmSchedulerId::primary()).expect("protocol route");
    let wake = VmSocketReadinessWake {
        route,
        readable: true,
        writable: false,
        closed: true,
    };

    assert_eq!(wake.route, route);
    assert!(wake.readable);
    assert!(!wake.writable);
    assert!(wake.closed);
}

#[test]
fn protocol_completion_origin_rejects_foreign_and_ambient_threads() {
    let topology = VmSchedulerTopology::new(2).expect("topology");
    let mut schedulers = topology.schedulers();
    let primary = schedulers.next().expect("primary scheduler");
    let secondary = schedulers.next().expect("secondary scheduler");
    let route = next_protocol_task_route(secondary).expect("protocol route");

    let ambient = route
        .validate_completion_origin()
        .expect_err("ambient thread must be rejected");
    assert!(
        ambient.contains("outside a protocol scheduler"),
        "{ambient}"
    );
    let foreign = with_protocol_scheduler_for_test(primary, || route.validate_completion_origin())
        .expect_err("foreign protocol owner must be rejected");
    assert!(foreign.contains("not scheduler 0"), "{foreign}");
    with_protocol_scheduler_for_test(secondary, || route.validate_completion_origin())
        .expect("exact protocol owner");
}

#[test]
fn repeated_task_wakes_queue_one_vm_poll() {
    let poll = Poll::new().expect("poll");
    let queue = Arc::new(ConcurrentQueue::bounded(4));
    let wake = Arc::new(VmProtocolTaskWake {
        token: AtomicUsize::new(FIRST_TASK_TOKEN),
        scheduled: AtomicBool::new(false),
        owner: Arc::new(VmProtocolOwnerWake {
            scheduler: VmSchedulerId::primary(),
            registry: poll.registry().try_clone().expect("registry clone"),
            queue: Arc::clone(&queue),
            poll_waker: Arc::new(
                MioWaker::new(poll.registry(), TASK_WAKE_TOKEN).expect("poll waker"),
            ),
        }),
    });

    wake.wake_by_ref();
    wake.wake_by_ref();

    assert!(wake.scheduled.load(Ordering::Acquire));
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.pop(), Ok(Token(FIRST_TASK_TOKEN)));
}

#[test]
fn owner_local_task_wake_queues_without_signaling_the_readiness_poller() {
    let mut poll = Poll::new().expect("poll");
    let queue = Arc::new(ConcurrentQueue::bounded(4));
    let scheduler = VmSchedulerId::primary();
    let wake = Arc::new(VmProtocolTaskWake {
        token: AtomicUsize::new(FIRST_TASK_TOKEN),
        scheduled: AtomicBool::new(false),
        owner: Arc::new(VmProtocolOwnerWake {
            scheduler,
            registry: poll.registry().try_clone().expect("registry clone"),
            queue: Arc::clone(&queue),
            poll_waker: Arc::new(
                MioWaker::new(poll.registry(), TASK_WAKE_TOKEN).expect("poll waker"),
            ),
        }),
    });

    with_protocol_scheduler_for_test(scheduler, || wake.wake_by_ref());

    let mut events = Events::with_capacity(1);
    poll.poll(&mut events, Some(std::time::Duration::ZERO))
        .expect("poll local wake events");
    assert!(events.is_empty());
    assert!(queue.is_empty());
    assert_eq!(pop_owner_local_scheduled(), Some(Token(FIRST_TASK_TOKEN)));
}

#[cfg(unix)]
#[test]
fn bound_listener_is_nonblocking_for_the_vm_acceptor() {
    let listener = bind_protocol_listener("127.0.0.1", 0).expect("listener");
    let error = listener
        .accept()
        .expect_err("empty listener must not block");

    assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
}

#[test]
fn least_loaded_admission_rotates_ties_and_skips_full_shards() {
    let topology = VmSchedulerTopology::new(3).expect("topology");
    let mut polls = Vec::new();
    let mut ingresses = Vec::new();
    for scheduler in topology.schedulers() {
        let shard_poll = Poll::new().expect("shard poll");
        let acceptor_poll = Poll::new().expect("acceptor poll");
        let ingress = Arc::new(VmProtocolShardIngress {
            scheduler,
            sockets: ConcurrentQueue::bounded(MAX_TASKS_PER_SHARD),
            load: AtomicUsize::new(0),
            owner_parked: AtomicBool::new(false),
            poll_waker: Arc::new(
                MioWaker::new(shard_poll.registry(), TASK_WAKE_TOKEN).expect("shard waker"),
            ),
            capacity: Arc::new(VmProtocolCapacity::default()),
        });
        polls.push((shard_poll, acceptor_poll));
        ingresses.push(ingress);
    }

    ingresses[0].load.store(2, Ordering::Release);
    ingresses[1].load.store(0, Ordering::Release);
    ingresses[2].load.store(1, Ordering::Release);
    assert_eq!(least_loaded_shard(&ingresses, 0), Some(1));

    for ingress in &ingresses {
        ingress.load.store(0, Ordering::Release);
    }
    assert_eq!(least_loaded_shard(&ingresses, 2), Some(2));

    ingresses[0].load.store(1, Ordering::Release);
    assert_eq!(least_loaded_shard(&ingresses, 2), Some(2));

    ingresses[2]
        .load
        .store(MAX_TASKS_PER_SHARD, Ordering::Release);
    assert_eq!(least_loaded_shard(&ingresses, 2), Some(1));

    for ingress in &ingresses {
        ingress.load.store(0, Ordering::Release);
    }
    assert_eq!(admission_target(&ingresses, 0, 2), Some(0));
    ingresses[0].load.store(1, Ordering::Release);
    assert_eq!(admission_target(&ingresses, 0, 2), Some(2));
    ingresses[0].load.store(2, Ordering::Release);
    assert_eq!(admission_target(&ingresses, 0, 2), Some(2));
    ingresses[0]
        .load
        .store(MAX_TASKS_PER_SHARD, Ordering::Release);
    assert_eq!(admission_target(&ingresses, 0, 2), Some(2));

    ingresses[0]
        .load
        .store(MAX_TASKS_PER_SHARD, Ordering::Release);
    ingresses[1].load.store(1, Ordering::Release);
    ingresses[2].load.store(0, Ordering::Release);
    assert_eq!(sampled_loaded_shard(&ingresses, 1), Some(2));

    ingresses[1]
        .load
        .store(MAX_TASKS_PER_SHARD, Ordering::Release);
    ingresses[2]
        .load
        .store(MAX_TASKS_PER_SHARD, Ordering::Release);
    assert_eq!(sampled_loaded_shard(&ingresses, 1), None);
    drop(polls);
}

#[test]
fn task_completion_wakes_only_a_capacity_blocked_acceptor() {
    let shard_poll = Poll::new().expect("shard poll");
    let mut acceptor_poll = Poll::new().expect("acceptor poll");
    let capacity = Arc::new(VmProtocolCapacity::default());
    let capacity_waker = Arc::new(
        MioWaker::new(acceptor_poll.registry(), ACCEPTOR_CAPACITY_TOKEN).expect("acceptor waker"),
    );
    capacity.register(&capacity_waker).expect("register waker");
    let ingress = VmProtocolShardIngress {
        scheduler: VmSchedulerId::primary(),
        sockets: ConcurrentQueue::bounded(MAX_TASKS_PER_SHARD),
        load: AtomicUsize::new(1),
        owner_parked: AtomicBool::new(false),
        poll_waker: Arc::new(
            MioWaker::new(shard_poll.registry(), TASK_WAKE_TOKEN).expect("shard waker"),
        ),
        capacity: Arc::clone(&capacity),
    };
    let mut events = Events::with_capacity(2);

    ingress.complete();
    acceptor_poll
        .poll(&mut events, Some(std::time::Duration::ZERO))
        .expect("idle acceptor poll");
    assert!(events.is_empty());

    ingress.load.store(1, Ordering::Release);
    capacity.waiting.store(true, Ordering::Release);
    ingress.complete();
    acceptor_poll
        .poll(&mut events, Some(std::time::Duration::from_secs(1)))
        .expect("capacity acceptor poll");
    assert!(events
        .iter()
        .any(|event| event.token() == ACCEPTOR_CAPACITY_TOKEN));
    assert!(!capacity.waiting.load(Ordering::Acquire));
}

#[test]
fn remote_ingress_publishes_before_waking_a_parked_owner() {
    let mut shard_poll = Poll::new().expect("shard poll");
    let ingress = VmProtocolShardIngress {
        scheduler: VmSchedulerId::primary(),
        sockets: ConcurrentQueue::bounded(MAX_TASKS_PER_SHARD),
        load: AtomicUsize::new(0),
        owner_parked: AtomicBool::new(true),
        poll_waker: Arc::new(
            MioWaker::new(shard_poll.registry(), TASK_WAKE_TOKEN).expect("shard waker"),
        ),
        capacity: Arc::new(VmProtocolCapacity::default()),
    };
    let listener = std_net::TcpListener::bind(("127.0.0.1", 0)).expect("listener");
    let client = std_net::TcpStream::connect(listener.local_addr().expect("address"))
        .expect("client connection");
    let (server, _) = listener.accept().expect("server connection");

    ingress
        .admit(TcpStream::from_std(server), true)
        .expect("remote admission");

    let mut events = Events::with_capacity(1);
    shard_poll
        .poll(&mut events, Some(std::time::Duration::from_secs(1)))
        .expect("parked owner wake");
    assert_eq!(ingress.sockets.len(), 1);
    assert_eq!(ingress.load(), 1);
    assert!(events.iter().any(|event| event.token() == TASK_WAKE_TOKEN));
    drop(client);
}
