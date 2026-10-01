use super::*;
use std::future::{pending, ready};
use std::task::{Context, Poll, Waker};

#[test]
fn host_adapter_preserves_connection_and_capacity_diagnostics() {
    let capacity = NonZeroUsize::new(1).unwrap();
    let mut failed = Box::pin(drive_connection(capacity, |_| ready(Err("broken"))));
    assert_eq!(
        failed
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Err("Hyper HTTP/2 TLS connection failed: broken".into()))
    );

    let mut overflowed = Box::pin(drive_connection(capacity, |spawner| {
        spawner.spawn(Box::pin(pending())).unwrap();
        assert!(spawner.spawn(Box::pin(pending())).is_err());
        pending::<Result<(), String>>()
    }));
    assert_eq!(
        overflowed
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Err(
            "error[vm.http2.stream_pressure]: owner-local HTTP/2 task limit exceeded".into()
        ))
    );
}
