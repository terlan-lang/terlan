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

/// Proves a child observes its parent without propagating cancellation upward.
#[test]
fn child_cancellation_is_local_and_parent_cancellation_is_inherited() {
    let parent = NativeBoundaryCancellationToken::new();
    let child = parent.child();
    let grandchild = child.child();
    let sibling = parent.child();

    child.cancel();
    assert!(child.is_cancelled());
    assert!(grandchild.is_cancelled());
    assert!(!parent.is_cancelled());
    assert!(!sibling.is_cancelled());

    parent.cancel();
    assert!(sibling.is_cancelled());
}

/// Proves root cancellation reaches descendants without intermediate polling.
#[test]
fn root_cancellation_reaches_nested_children() {
    let root = NativeBoundaryCancellationToken::new();
    let child = root.child();
    let grandchild = child.child();

    root.cancel();

    assert!(grandchild.is_cancelled());
}
