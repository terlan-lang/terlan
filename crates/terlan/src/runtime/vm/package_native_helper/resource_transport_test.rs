//! Idle worker termination must not strand another owner's queued event.
use super::*;
use std::io::{self, Read};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Barrier,
};
use std::task::Wake;

#[test]
fn capacity_rejection_is_distinct_from_lost_database_worker() {
    use super::super::{postgres::Projection, postgres_transport};
    use crate::runtime::vm::{
        execution_shard_epoch::{
            VmShardEpochOperation, VmShardOperationId, VmShardOperationKind, VmShardReplayPolicy,
        },
        execution_shard_protocol::VmShardEpoch,
        process::{VmProcessSource, VmProcessTable},
    };

    let mut processes = VmProcessTable::default();
    let mut workers = postgres_transport::Workers::<()>::default();
    let owners = (0..17)
        .map(|_| processes.spawn_root(VmProcessSource::new("app.Resource", "call", 0)))
        .collect::<Vec<_>>();
    for owner in &owners[..16] {
        workers.owners.insert(owner.as_u64(), None);
    }
    let pending = |owner| Pending {
        owner,
        payload: (),
        projection: Projection::Unit,
        context: VmCapabilityRequestContext::new(
            VmCapabilityId::new("postgres").unwrap(),
            VmShardEpochOperation::new(
                VmShardOperationId::new(1).unwrap(),
                VmShardEpoch::new(1).unwrap(),
                VmShardOperationKind::CapabilityCompletion,
                VmShardReplayPolicy::AtMostOnce,
            ),
        )
        .unwrap(),
    };
    let capacity = workers
        .submit("unused", vec![], pending(owners[16]))
        .unwrap_err();
    assert!(capacity
        .to_string()
        .starts_with("error[postgres.resource_limit]:"));
    let lost = workers
        .submit("unused", vec![], pending(owners[0]))
        .unwrap_err();
    assert!(lost
        .to_string()
        .starts_with("error[postgres.indeterminate]:"));
    assert_eq!(workers.owners.len(), 16);
    assert!(workers.owners.values().all(Option::is_none));
    workers.close_owner(owners[0].as_u64());
    assert_eq!(workers.owners.len(), 15);
}

struct EndOfStream(Arc<Barrier>);
impl Read for EndOfStream {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        self.0.wait();
        Ok(0)
    }
}
struct Notifications(AtomicUsize, std::thread::Thread);
impl Wake for Notifications {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
        self.1.unpark();
    }
}
#[test]
fn losing_an_idle_worker_drains_other_ready_owners_in_the_same_poll() {
    let barrier = Arc::new(Barrier::new(3));
    let notifications = Arc::new(Notifications(AtomicUsize::new(0), std::thread::current()));
    let waker = Waker::from(notifications.clone());
    let mut workers = Workers::<(), ()>::new(Policy {
        capability: "package-native",
        worker_class: "fast",
        identity: "resource-test",
        error_code: "resource_worker.lost",
        capacity_error_code: "resource_worker.resource_limit",
        loss_context: "resources revoked",
    });
    for owner in [1, 2] {
        let identity = VmCapabilityWorkerIdentity::new(
            VmCapabilityWorkerId::new(format!("test-{owner}")).unwrap(),
            VmCapabilityWorkerGeneration::new(1).unwrap(),
        );
        let client = VmCapabilityWorkerClient::from_test_streams(
            identity,
            &["postgres"],
            1,
            io::sink(),
            EndOfStream(barrier.clone()),
        )
        .unwrap();
        let pool =
            VmCapabilityWorkerPool::new(vec![VmCapabilityWorkerPoolSlot::new(client, 1).unwrap()])
                .unwrap();
        let pump = VmCapabilityWorkerEventPump::new(pool);
        pump.register_event_waker(&waker);
        workers.owners.insert(
            owner,
            Some(Worker {
                pump,
                deadline: None,
            }),
        );
    }
    barrier.wait();
    let deadline = Instant::now() + Duration::from_secs(5);
    while notifications.0.load(Ordering::SeqCst) != 2 {
        assert!(Instant::now() < deadline, "worker EOF was not published");
        std::thread::park_timeout(Duration::from_millis(10));
    }
    assert!(workers.poll().unwrap().is_none());
    assert!(workers.owners.values().all(Option::is_none));
}
