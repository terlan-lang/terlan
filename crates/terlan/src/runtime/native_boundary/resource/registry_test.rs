//! VM-owned values share adapter handle validation without erased Rust types.

use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[test]
fn typed_values_require_their_real_owner_and_generation() {
    let mut store = ResourceRegistry::new();
    let handle = store.insert_for_owner(17, "profile".to_string()).unwrap();
    assert_eq!(store.get_for_owner(handle, 17).unwrap(), "profile");
    assert_eq!(
        store.get_for_owner(handle, 18).unwrap_err().code(),
        "resource.owner"
    );
    let forged = NativeBoundaryHandle {
        generation: handle.generation + 1,
        ..handle
    };
    assert_eq!(
        store.get_for_owner(forged, 17).unwrap_err().code(),
        "resource.stale_handle"
    );
    assert_eq!(
        store.get_for_owner(forged, 18).unwrap_err().code(),
        "resource.stale_handle"
    );
}

#[test]
fn mutation_checks_authority_before_exposing_a_mutable_reference() {
    let mut store = ResourceRegistry::new();
    let handle = store.insert_for_owner(17, 1).unwrap();
    assert_eq!(
        store.get_mut_for_owner(handle, 18).unwrap_err().code(),
        "resource.owner"
    );
    let forged = NativeBoundaryHandle {
        generation: 2,
        ..handle
    };
    assert_eq!(
        store.get_mut_for_owner(forged, 17).unwrap_err().code(),
        "resource.stale_handle"
    );
    assert_eq!(*store.get_for_owner(handle, 17).unwrap(), 1);
    *store.get_mut_for_owner(handle, 17).unwrap() = 2;
    assert_eq!(*store.get_for_owner(handle, 17).unwrap(), 2);
    store.dispose_owner(17);
    assert!(store.get_mut_for_owner(handle, 17).is_err());
}

#[test]
fn disposed_tokens_never_alias_new_values() {
    let mut store = ResourceRegistry::new();
    let first = store.insert_for_owner(17, 1).unwrap();
    assert!(store.dispose_for_owner(first, 18).is_err());
    assert_eq!(store.get_for_owner(first, 17).unwrap(), &1);
    store.dispose_for_owner(first, 17).unwrap();
    let second = store.insert_for_owner(17, 2).unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(
        store.get_for_owner(first, 17).unwrap_err().code(),
        "resource.stale_handle"
    );
    assert_eq!(store.get_for_owner(second, 17).unwrap(), &2);
}

#[derive(Debug)]
struct Counted(Arc<AtomicUsize>);

impl Drop for Counted {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn cleanup_drops_nonclone_resources_only_for_the_completed_owner() {
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut store = ResourceRegistry::new();
    let first = store
        .insert_for_owner(17, Counted(dropped.clone()))
        .unwrap();
    let second = store
        .insert_for_owner(18, Counted(dropped.clone()))
        .unwrap();
    assert_eq!(store.dispose_owner(17), 1);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
    assert!(store.get_for_owner(first, 17).is_err());
    assert!(store.get_for_owner(second, 18).is_ok());
    assert_eq!(store.dispose_owner(17), 0);
    drop(store);
    assert_eq!(dropped.load(Ordering::SeqCst), 2);
}
