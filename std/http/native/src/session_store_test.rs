use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
struct State {
    next: u64,
    values: BTreeMap<u64, BTreeMap<String, String>>,
    fail: &'static str,
}

#[test]
fn source_selected_lifetimes_are_independent_and_invalid_renewal_cannot_mutate_state() {
    let mut store = SessionStore::new(State::default(), RecoveryPolicy::CreateLocalReplacement);
    assert!(store
        .create("", 0)
        .unwrap_err()
        .to_string()
        .contains("TTL must be greater than 0"));
    assert!(store.is_empty());
    assert_eq!(store.resources.next, 0);
    let short = store.create("", 2).unwrap();
    let long = store.create("", 20).unwrap();
    store.set(&short, "key", "kept").unwrap();
    store.registry.advance_ticks(1);
    let before = store.registry.entries()[&short].clone();
    assert!(store.rotate(&short, 0).is_err());
    assert_eq!(store.registry.entries()[&short], before);
    assert_eq!(store.resources.next, 2);
    assert_eq!(store.lookup(&long).unwrap(), Some(long.clone()));
    assert_eq!(store.registry.entries()[&long].expires_at_tick, 20);
    let renewed = store.rotate(&short, 4).unwrap();
    assert_eq!(store.registry.entries()[&renewed].expires_at_tick, 5);
    assert_eq!(store.get(&renewed, "key").unwrap(), Some("kept".into()));
    assert!(!store.is_live(&short).unwrap());
    store.registry.advance_ticks(4);
    store.maintain(10).unwrap();
    assert!(!store.is_live(&renewed).unwrap());
    assert!(store.is_live(&long).unwrap());
    store.registry.advance_ticks(15);
    store.maintain(10).unwrap();
    assert!(store.is_empty());
    assert!(store.resources.values.is_empty());
}

fn failure() -> BoundaryError {
    BoundaryError::message(ErrorDomain::VmRuntime, "fixture resource", "unavailable")
}

impl State {
    fn check(&self, operation: &str) -> Result<(), BoundaryError> {
        if self.fail == operation {
            Err(failure())
        } else {
            Ok(())
        }
    }
}

impl ActorStateStore<String> for State {
    type Handle = u64;
    fn create(&mut self, owner: &str, name: &str) -> Result<u64, BoundaryError> {
        self.check("create")?;
        assert_eq!(owner, "std.http.Session");
        assert!(name.starts_with("http_session:"));
        self.next += 1;
        self.values.insert(self.next, BTreeMap::new());
        Ok(self.next)
    }
    fn check_live(&self, handle: &u64) -> Result<(), BoundaryError> {
        self.check("live")?;
        self.values
            .contains_key(handle)
            .then_some(())
            .ok_or_else(failure)
    }
    fn release(&mut self, handle: &u64) -> Result<(), BoundaryError> {
        self.check("release")?;
        self.values.remove(handle);
        Ok(())
    }
    fn read(&self, handle: &u64, key: &str) -> Result<Option<String>, BoundaryError> {
        self.check("read")?;
        Ok(self.values[handle].get(key).cloned())
    }
    fn write(&mut self, handle: &u64, key: &str, value: String) -> Result<(), BoundaryError> {
        self.check("write")?;
        self.values
            .get_mut(handle)
            .unwrap()
            .insert(key.into(), value);
        Ok(())
    }
    fn delete(&mut self, handle: &u64, key: &str) -> Result<(), BoundaryError> {
        self.check("delete")?;
        self.values.get_mut(handle).unwrap().remove(key);
        Ok(())
    }
}

#[test]
fn session_store_executes_all_storage_policy_over_host_actor_state() {
    let mut store = SessionStore::new(State::default(), RecoveryPolicy::CreateLocalReplacement);
    assert!(store.is_empty());
    let first = store.create("", 1).unwrap();
    let second = store.create("", 1).unwrap();
    assert_ne!(first, second);
    assert_eq!(store.lookup(&first).unwrap(), Some(first.clone()));
    assert_eq!(store.get(&first, "").unwrap(), None);
    store.set(&first, "", "").unwrap();
    store.set(&first, " key\0 ", " value\u{2003} ").unwrap();
    assert_eq!(store.get(&first, "").unwrap(), Some("".into()));
    assert_eq!(store.get(&second, "").unwrap(), None);
    let rotated = store.rotate(&first, 1).unwrap();
    assert_ne!(rotated, first);
    assert_eq!(
        store.get(&rotated, " key\0 ").unwrap(),
        Some(" value\u{2003} ".into())
    );
    assert!(!store.is_live(&first).unwrap());
    assert!(store.get(&first, "").is_err());
    assert!(store.set(&first, "", "").is_err());
    assert!(store.delete(&first, "").is_err());
    assert!(store.rotate(&first, 1).is_err());
    assert!(store.expire(&first).is_err());
    store.delete(&rotated, "").unwrap();
    store.delete(&rotated, "").unwrap();
    assert_eq!(store.get(&rotated, "").unwrap(), None);
    store.expire(&rotated).unwrap();
    store.expire(&second).unwrap();
    assert!(store.is_empty());
    assert!(store.resources.values.is_empty());
}

#[test]
fn session_store_retains_failed_cleanup_and_never_replays_failed_writes() {
    let mut store = SessionStore::new(State::default(), RecoveryPolicy::CreateLocalReplacement);
    store.resources.fail = "create";
    assert!(store.create("", 1).is_err());
    assert!(store.is_empty());
    store.resources.fail = "";
    let identity = store.create("", 1).unwrap();
    store.resources.fail = "read";
    assert_eq!(store.get(&identity, "key"), Err(failure()));
    store.resources.fail = "write";
    assert_eq!(store.set(&identity, "key", "value"), Err(failure()));
    store.resources.fail = "delete";
    assert_eq!(store.delete(&identity, "key"), Err(failure()));
    store.resources.fail = "release";
    assert!(store.expire(&identity).is_err());
    assert!(store.is_live(&identity).unwrap());
    store.resources.fail = "";
    assert_eq!(store.get(&identity, "key").unwrap(), None);
    store.expire(&identity).unwrap();
    assert!(store.is_empty());
}

#[test]
fn session_store_reclaims_dead_actors_and_obeys_fail_closed_recovery() {
    for recovery in [
        RecoveryPolicy::CreateLocalReplacement,
        RecoveryPolicy::FailClosed,
    ] {
        let mut store = SessionStore::new(State::default(), recovery);
        let identity = store.create("", 1).unwrap();
        store.resources.values.clear();
        assert!(!store.is_live(&identity).unwrap());
        assert!(store.get(&identity, "").is_err());
        assert!(store.is_empty());
        let replacement = store.lookup(&identity);
        match recovery {
            RecoveryPolicy::CreateLocalReplacement => assert_eq!(replacement.unwrap(), None),
            RecoveryPolicy::FailClosed => assert!(replacement.is_err()),
        }
    }
}

#[test]
fn session_store_clock_and_bounded_expiry_preserve_cleanup_retry() {
    let mut store = SessionStore::new(State::default(), RecoveryPolicy::FailClosed);
    store.start_clock().unwrap();
    let identity = store.create("", 1).unwrap();
    store.create("", 1).unwrap();
    store.registry.advance_ticks(1);
    assert!(!store.is_live(&identity).unwrap());
    store.maintain(0).unwrap();
    assert_eq!(store.registry.entries().len(), 2);
    store.resources.fail = "release";
    assert!(store.maintain(1).is_err());
    assert_eq!(store.registry.entries().len(), 2);
    store.resources.fail = "";
    store.maintain(1).unwrap();
    assert_eq!(store.registry.entries().len(), 1);
    store.maintain(1).unwrap();
    assert!(store.is_empty());
    assert!(store.resources.values.is_empty());
}

#[test]
fn lookup_never_allocates_and_cleanup_failure_cannot_become_absence() {
    let mut store = SessionStore::new(State::default(), RecoveryPolicy::CreateLocalReplacement);
    for identity in ["", "missing", " \u{2003} "] {
        assert_eq!(store.lookup(identity).unwrap(), None);
        assert!(store.is_empty());
        assert_eq!(store.resources.next, 0);
    }
    let identity = store.create("untrusted identity", 1).unwrap();
    assert_ne!(identity, "untrusted identity");
    assert_eq!(store.lookup(&identity).unwrap(), Some(identity.clone()));
    store.registry.advance_ticks(1);
    store.resources.fail = "release";
    assert!(store.lookup(&identity).is_err());
    assert!(!store.is_empty());
    assert_eq!(store.resources.next, 1);
    store.resources.fail = "";
    assert_eq!(store.lookup(&identity).unwrap(), None);
    assert!(store.is_empty());
    assert_eq!(store.resources.next, 1);
    let replacement = store.create(&identity, 1).unwrap();
    assert_ne!(replacement, identity);
    assert_eq!(store.resources.next, 2);
}
