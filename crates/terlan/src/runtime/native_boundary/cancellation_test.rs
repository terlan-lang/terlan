//! Cooperative cancellation token tests.

use std::thread;

use super::NativeBoundaryCancellationToken;

/// Proves cancellation is visible across cloned thread-owned tokens.
#[test]
fn cancellation_is_monotonic_and_cross_thread_visible() {
    let token = NativeBoundaryCancellationToken::new();
    let observer = token.clone();
    assert!(!observer.is_cancelled());

    thread::spawn(move || token.cancel())
        .join()
        .expect("cancellation thread");

    assert!(observer.is_cancelled());
    observer.cancel();
    assert!(observer.is_cancelled());
}
