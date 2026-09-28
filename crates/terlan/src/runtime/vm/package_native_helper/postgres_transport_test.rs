//! Idle worker termination must not strand another owner's queued event.
use super::*;
use std::io::{self, Read};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Barrier,
};
use std::task::Wake;

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
    let mut workers = Workers::<()>::default();
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
