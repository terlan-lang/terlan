use std::sync::Arc;
use std::task::{Wake, Waker};

use super::*;

struct NoopWake;

impl Wake for NoopWake {
    fn wake(self: Arc<Self>) {}
}

#[test]
fn drop_removes_owner_local_deadline() {
    let waker = Waker::from(Arc::new(NoopWake));
    let mut context = Context::from_waker(&waker);
    let mut sleep = Box::pin(protocol_sleep_until(
        Instant::now() + Duration::from_secs(60),
    ));
    assert_eq!(sleep.as_mut().poll(&mut context), Poll::Pending);
    assert!(next_protocol_timer_timeout(Instant::now()).is_some());
    drop(sleep);
    assert_eq!(next_protocol_timer_timeout(Instant::now()), None);
}
