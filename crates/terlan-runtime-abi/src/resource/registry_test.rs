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
    let forged = NativeResourceHandle {
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
    let forged = NativeResourceHandle {
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

#[test]
fn insert_rejects_id_overflow_without_mutation() {
    let mut store = ResourceRegistry::new();
    let existing = store.insert_for_owner(17, 42).unwrap();
    store.next_id = u64::MAX;
    assert_eq!(
        store.insert_for_owner(17, 43).unwrap_err().code(),
        "resource.id_overflow"
    );
    assert_eq!(store.resources.len(), 1);
    assert_eq!(*store.get_for_owner(existing, 17).unwrap(), 42);
    assert_eq!(store.next_id, u64::MAX);
}

#[test]
fn trusted_access_still_checks_liveness_and_system_owner_has_no_override() {
    let mut store = ResourceRegistry::default();
    let system = store.insert(11).unwrap();
    let actor = store.insert_for_owner(7, 22).unwrap();
    assert_eq!(*store.get(system).unwrap(), 11);
    *store.get_mut(system).unwrap() = 12;
    assert_eq!(
        *store.get_for_owner(system, SYSTEM_RESOURCE_OWNER).unwrap(),
        12
    );
    let denied = store.dispose(actor).unwrap_err();
    assert_eq!(denied.code(), "resource.owner");
    assert!(denied
        .message()
        .contains("belongs to process 7, not process 0"));
    assert_eq!(*store.get(actor).unwrap(), 22);
    store.dispose(system).unwrap();
    for handle in [
        system,
        NativeResourceHandle {
            id: 0,
            generation: 1,
        },
        NativeResourceHandle {
            generation: 0,
            ..actor
        },
        NativeResourceHandle {
            generation: u64::MAX,
            ..actor
        },
    ] {
        assert_eq!(
            store.get(handle).unwrap_err().code(),
            "resource.stale_handle"
        );
        assert_eq!(
            store.get_mut(handle).unwrap_err().code(),
            "resource.stale_handle"
        );
        assert_eq!(
            store.dispose_for_owner(handle, 7).unwrap_err().code(),
            "resource.stale_handle"
        );
    }
    assert_eq!(*store.get_for_owner(actor, 7).unwrap(), 22);
}

#[test]
fn mixed_owner_cleanup_and_forged_access_never_change_survivors() {
    let mut store = ResourceRegistry::new();
    let mut handles = Vec::new();
    for value in 0..96 {
        let owner = value % 3;
        handles.push((store.insert_for_owner(owner, value).unwrap(), owner, value));
    }
    for (handle, owner, value) in &handles {
        for impostor in 0..3 {
            if impostor != *owner {
                assert!(store.get_for_owner(*handle, impostor).is_err());
                assert!(store.get_mut_for_owner(*handle, impostor).is_err());
                assert!(store.dispose_for_owner(*handle, impostor).is_err());
            }
        }
        assert_eq!(store.get_for_owner(*handle, *owner).unwrap(), value);
    }
    assert_eq!(store.dispose_owner(1), 32);
    assert_eq!(store.dispose_owner(1), 0);
    for (handle, owner, value) in &handles {
        if *owner == 1 {
            assert!(store.get_for_owner(*handle, *owner).is_err());
        } else {
            *store.get_mut_for_owner(*handle, *owner).unwrap() += 1;
            assert_eq!(*store.get_for_owner(*handle, *owner).unwrap(), value + 1);
        }
    }
    let replacement = store.insert_for_owner(1, 100).unwrap();
    assert!(handles
        .iter()
        .all(|(handle, _, _)| handle.id != replacement.id));
    assert_eq!(store.dispose_owner(0), 32);
    assert_eq!(store.dispose_owner(2), 32);
    assert_eq!(*store.get_for_owner(replacement, 1).unwrap(), 100);
}

#[test]
fn disposal_releases_owned_payloads_exactly_once() {
    use std::{cell::Cell, rc::Rc};

    struct Payload(Rc<Cell<usize>>);
    impl Drop for Payload {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    let released = Rc::new(Cell::new(0));
    let mut store = ResourceRegistry::new();
    let first = store
        .insert_for_owner(1, Payload(released.clone()))
        .unwrap();
    store
        .insert_for_owner(1, Payload(released.clone()))
        .unwrap();
    store
        .insert_for_owner(2, Payload(released.clone()))
        .unwrap();

    assert!(store.dispose_for_owner(first, 2).is_err());
    assert_eq!(released.get(), 0);
    store.dispose_for_owner(first, 1).unwrap();
    assert_eq!(released.get(), 1);
    assert!(store.dispose_for_owner(first, 1).is_err());
    assert_eq!(released.get(), 1);
    assert_eq!(store.dispose_owner(1), 1);
    assert_eq!(released.get(), 2);
    assert_eq!(store.dispose_owner(1), 0);
    assert_eq!(released.get(), 2);

    store.next_id = u64::MAX;
    assert!(store
        .insert_for_owner(2, Payload(released.clone()))
        .is_err());
    assert_eq!(released.get(), 3);
    drop(store);
    assert_eq!(released.get(), 4);
}
