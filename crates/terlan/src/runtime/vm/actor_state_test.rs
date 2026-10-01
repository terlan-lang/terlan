use super::*;

#[test]
fn actor_state_preserves_exact_keys_values_and_reclaims_resources() {
    let mut store = VmActorStateStore::default();
    let first = store.create("fixture.Owner", "first").unwrap();
    let second = store.create("fixture.Owner", "second").unwrap();
    for (key, value) in [("", ""), (" key\0 ", "\nvalue\u{2003}"), ("key", "other")] {
        assert_eq!(store.read(&first, key).unwrap(), None);
        store.write(&first, key, value.into()).unwrap();
        assert_eq!(store.read(&first, key).unwrap(), Some(value.into()));
        assert_eq!(store.read(&second, key).unwrap(), None);
    }
    store.write(&first, "key", "replacement".into()).unwrap();
    assert_eq!(
        store.read(&first, "key").unwrap(),
        Some("replacement".into())
    );
    for _ in 0..2 {
        store.delete(&first, "key").unwrap();
    }
    assert_eq!(store.read(&first, "key").unwrap(), None);
    store.release(&first).unwrap();
    store.release(&first).unwrap();
    assert!(store.check_live(&first).is_err());
    assert!(store.read(&first, "").is_err());
    assert!(store.write(&first, "", "".into()).is_err());
    assert!(store.delete(&first, "").is_err());
    assert_eq!(store.tables.snapshots().len(), 1);
    store.release(&second).unwrap();
    assert!(store.tables.snapshots().is_empty());
}

#[test]
fn actor_state_rejects_foreign_handles_even_when_numeric_ids_collide() {
    let mut first = VmActorStateStore::default();
    let mut second = VmActorStateStore::default();
    let a = first.create("one", "state").unwrap();
    let b = second.create("two", "state").unwrap();
    assert_eq!(a.actor, b.actor);
    assert_eq!(a.table, b.table);
    for error in [
        second.check_live(&a).unwrap_err(),
        second.read(&a, "key").unwrap_err(),
        second.write(&a, "key", "value".into()).unwrap_err(),
        second.delete(&a, "key").unwrap_err(),
        second.release(&a).unwrap_err(),
    ] {
        assert!(error.to_string().contains("foreign_handle"));
    }
    assert!(first.check_live(&a).is_ok());
    assert!(second.check_live(&b).is_ok());
    assert_eq!(second.tables.snapshots().len(), 1);
}

#[test]
fn actor_state_rejects_corrupt_values_and_releases_crashed_owners() {
    let mut store = VmActorStateStore::default();
    let handle = store.create("fixture", "state").unwrap();
    store
        .tables
        .insert(
            store.actors.processes(),
            handle.actor,
            handle.table,
            ReplValue::String("key".into()),
            ReplValue::Int(1),
        )
        .unwrap();
    assert!(store
        .read(&handle, "key")
        .unwrap_err()
        .to_string()
        .contains("String store contract"));
    store
        .actors
        .exit_actor(handle.actor, VmExitReason::Error("fixture crash".into()))
        .unwrap();
    assert!(store
        .check_live(&handle)
        .unwrap_err()
        .to_string()
        .contains("fixture crash"));
    store.release(&handle).unwrap();
    assert!(store.tables.snapshots().is_empty());
}
