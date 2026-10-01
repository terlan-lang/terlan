use super::*;
use std::cell::Cell;
use std::future::{pending, poll_fn, ready};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::Wake;

#[derive(Default)]
struct WakeCount(AtomicUsize);

impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

struct Dropped(Rc<Cell<usize>>);

impl Drop for Dropped {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn capacity(value: usize) -> NonZeroUsize {
    NonZeroUsize::new(value).unwrap()
}

fn poll<F: Future + Unpin>(future: &mut F) -> Poll<F::Output> {
    Pin::new(future).poll(&mut Context::from_waker(Waker::noop()))
}

#[test]
fn empty_group_completes_and_retained_spawner_is_closed() {
    let mut retained = None;
    let mut group = LocalTaskGroup::new(capacity(1), |spawn| {
        retained = Some(spawn);
        ready(Ok::<_, &'static str>(()))
    });
    assert_eq!(poll(&mut group), Poll::Ready(Ok(())));
    assert_eq!(
        retained.as_ref().unwrap().spawn(Box::pin(async {})),
        Err(SpawnError::Closed)
    );
    drop(group);
    assert_eq!(
        retained.unwrap().spawn(Box::pin(async {})),
        Err(SpawnError::Closed)
    );
}

#[test]
fn completed_root_waits_for_children_without_polling_them_twice() {
    let polls = Rc::new(Cell::new(0));
    let child_polls = Rc::clone(&polls);
    let mut group = LocalTaskGroup::new(capacity(1), |spawn| {
        spawn
            .spawn(Box::pin(poll_fn(move |_| {
                child_polls.set(child_polls.get() + 1);
                if child_polls.get() == 1 {
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })))
            .unwrap();
        ready(Ok::<_, &'static str>(()))
    });
    assert_eq!(poll(&mut group), Poll::Pending);
    assert_eq!(polls.get(), 1);
    assert_eq!(poll(&mut group), Poll::Ready(Ok(())));
    assert_eq!(polls.get(), 2);
}

#[test]
fn active_child_counts_toward_capacity_during_reentrant_submission() {
    let dropped = Rc::new(Cell::new(0));
    let child_drop = Dropped(Rc::clone(&dropped));
    let mut group = LocalTaskGroup::new(capacity(1), |spawn| {
        let child_spawner = spawn.clone();
        spawn
            .spawn(Box::pin(async move {
                let _guard = child_drop;
                assert_eq!(
                    child_spawner.spawn(Box::pin(async {})),
                    Err(SpawnError::CapacityExceeded)
                );
                pending::<()>().await;
            }))
            .unwrap();
        pending::<Result<(), &'static str>>()
    });
    assert_eq!(
        poll(&mut group),
        Poll::Ready(Err(TaskGroupError::CapacityExceeded))
    );
    assert_eq!(dropped.get(), 1);
    assert_eq!(group.state.borrow().active, 0);
    assert!(group.state.borrow().queued.is_empty());
}

#[test]
fn overflow_before_poll_cancels_root_and_children_without_execution() {
    let dropped = Rc::new(Cell::new(0));
    let root_drop = Dropped(Rc::clone(&dropped));
    let child_drop = Dropped(Rc::clone(&dropped));
    let mut group = LocalTaskGroup::new(capacity(1), |spawn| {
        spawn
            .spawn(Box::pin(async move {
                let _guard = child_drop;
                panic!("overflowed child must never run");
            }))
            .unwrap();
        assert_eq!(
            spawn.spawn(Box::pin(async {})),
            Err(SpawnError::CapacityExceeded)
        );
        poll_fn(move |_| -> Poll<Result<(), &'static str>> {
            let _guard = &root_drop;
            panic!("overflowed root must never run");
        })
    });
    assert_eq!(
        poll(&mut group),
        Poll::Ready(Err(TaskGroupError::CapacityExceeded))
    );
    assert_eq!(dropped.get(), 2);
}

#[test]
fn overflow_during_root_poll_fails_immediately_without_polling_children() {
    let mut group = LocalTaskGroup::new(capacity(1), |spawn| async move {
        spawn
            .spawn(Box::pin(async { panic!("must be cancelled") }))
            .unwrap();
        assert_eq!(
            spawn.spawn(Box::pin(async {})),
            Err(SpawnError::CapacityExceeded)
        );
        Ok::<_, &'static str>(())
    });
    assert_eq!(
        poll(&mut group),
        Poll::Ready(Err(TaskGroupError::CapacityExceeded))
    );
}

#[test]
fn child_overflow_is_sticky_and_cancels_siblings_before_their_poll() {
    let mut group = LocalTaskGroup::new(capacity(2), |spawn| {
        let child_spawner = spawn.clone();
        spawn
            .spawn(Box::pin(async move {
                for _ in 0..2 {
                    assert_eq!(
                        child_spawner.spawn(Box::pin(async {})),
                        Err(SpawnError::CapacityExceeded)
                    );
                }
            }))
            .unwrap();
        spawn
            .spawn(Box::pin(async {
                panic!("sibling must be cancelled before execution");
            }))
            .unwrap();
        ready(Ok::<_, &'static str>(()))
    });
    assert_eq!(
        poll(&mut group),
        Poll::Ready(Err(TaskGroupError::CapacityExceeded))
    );
}

#[test]
fn nested_admission_wakes_owner_and_defers_new_child_until_next_turn() {
    let ran = Rc::new(Cell::new(false));
    let child_ran = Rc::clone(&ran);
    let mut group = LocalTaskGroup::new(capacity(2), |spawn| {
        let child_spawner = spawn.clone();
        spawn
            .spawn(Box::pin(async move {
                child_spawner
                    .spawn(Box::pin(async move {
                        child_ran.set(true);
                    }))
                    .unwrap();
            }))
            .unwrap();
        ready(Ok::<_, &'static str>(()))
    });
    let wake_count = Arc::new(WakeCount::default());
    let waker = Waker::from(Arc::clone(&wake_count));
    assert_eq!(
        Pin::new(&mut group).poll(&mut Context::from_waker(&waker)),
        Poll::Pending
    );
    assert!(!ran.get());
    assert!(wake_count.0.load(Ordering::SeqCst) > 0);
    assert_eq!(poll(&mut group), Poll::Ready(Ok(())));
    assert!(ran.get());
}

#[test]
fn completed_children_release_capacity_and_admission_uses_latest_waker() {
    let mut retained = None;
    let mut group = LocalTaskGroup::new(capacity(1), |spawn| {
        retained = Some(spawn.clone());
        spawn.spawn(Box::pin(async {})).unwrap();
        pending::<Result<(), &'static str>>()
    });
    let first = Arc::new(WakeCount::default());
    let second = Arc::new(WakeCount::default());
    for counter in [&first, &second] {
        let waker = Waker::from(Arc::clone(counter));
        assert!(Pin::new(&mut group)
            .poll(&mut Context::from_waker(&waker))
            .is_pending());
    }
    retained.unwrap().spawn(Box::pin(async {})).unwrap();
    assert_eq!(first.0.load(Ordering::SeqCst), 0);
    assert_eq!(second.0.load(Ordering::SeqCst), 1);
    assert_eq!(group.state.borrow().active, 1);
}

#[test]
fn root_failure_cancels_children_and_preserves_original_error() {
    let dropped = Rc::new(Cell::new(0));
    let guard = Dropped(Rc::clone(&dropped));
    let mut group = LocalTaskGroup::new(capacity(1), |spawn| {
        spawn
            .spawn(Box::pin(async move {
                let _guard = guard;
                pending::<()>().await;
            }))
            .unwrap();
        ready(Err("root failure"))
    });
    assert_eq!(
        poll(&mut group),
        Poll::Ready(Err(TaskGroupError::Root("root failure")))
    );
    assert_eq!(dropped.get(), 1);
}

#[test]
fn driver_drop_cancels_parked_work_and_reentrant_drop_cannot_resurrect_group() {
    struct Cleanup(LocalTaskSpawner, Rc<Cell<bool>>);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            assert_eq!(self.0.spawn(Box::pin(async {})), Err(SpawnError::Closed));
            self.1.set(true);
        }
    }
    let cleaned = Rc::new(Cell::new(false));
    let mut group = LocalTaskGroup::new(capacity(2), |spawn| {
        let cleanup = Cleanup(spawn.clone(), Rc::clone(&cleaned));
        spawn
            .spawn(Box::pin(async move {
                let _cleanup = cleanup;
                pending::<()>().await;
            }))
            .unwrap();
        pending::<Result<(), &'static str>>()
    });
    assert!(poll(&mut group).is_pending());
    drop(group);
    assert!(cleaned.get());
}
