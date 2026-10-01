use super::*;
use std::collections::BTreeSet;

#[derive(Default)]
struct Resources {
    next: u64,
    live: BTreeSet<u64>,
    released: Vec<u64>,
    fail_create: bool,
    fail_release: bool,
}

impl SessionResources<u64> for Resources {
    fn create(&mut self, _: &str) -> Result<u64, SessionError> {
        if self.fail_create {
            return Err(SessionError::Resource("allocation failed".into()));
        }
        self.next += 1;
        self.live.insert(self.next);
        Ok(self.next)
    }
    fn failure(&self, _: &str, value: &u64) -> Option<SessionError> {
        (!self.live.contains(value)).then(|| SessionError::Resource("resource exited".into()))
    }
    fn release(&mut self, value: &u64) -> Result<(), SessionError> {
        if self.fail_release {
            return Err(SessionError::Resource("cleanup failed".into()));
        }
        self.live.remove(value);
        self.released.push(*value);
        Ok(())
    }
}

fn registry() -> SessionRegistry<u64> {
    SessionRegistry::new(10, RecoveryPolicy::CreateLocalReplacement).unwrap()
}

#[test]
fn exact_identity_reuses_resource_without_sliding_expiry() {
    let mut registry = registry();
    let mut resources = Resources::default();
    let first = registry.acquire(None, &mut resources).unwrap();
    assert_eq!(first.id.len(), 43);
    assert_eq!(first.expires_at_tick, 10);
    registry.advance_ticks(9);
    assert_eq!(
        registry.acquire(Some(&first.id), &mut resources).unwrap(),
        first
    );
    for unknown in [String::new(), format!(" {} ", first.id), "missing".into()] {
        let other = registry.acquire(Some(&unknown), &mut resources).unwrap();
        assert_ne!(other.id, unknown);
        assert_ne!(other.value, first.value);
        assert_eq!(other.expires_at_tick, 19);
    }
    assert_eq!(resources.next, 4);
    registry.advance_ticks(1);
    assert!(!registry.is_live(&first.id, |_| true));
    assert_eq!(
        registry.expire_due(&mut resources).unwrap(),
        vec![first.id.clone()]
    );
    assert_eq!(resources.released, vec![first.value]);
    assert_eq!(registry.entries().len(), 3);
    let replacement = registry.acquire(Some(&first.id), &mut resources).unwrap();
    assert_ne!(replacement.id, first.id);
    assert_eq!(replacement.expires_at_tick, 20);
}

#[test]
fn fail_closed_cleans_stale_resources_but_allows_absent_identity() {
    let mut registry = SessionRegistry::new(1, RecoveryPolicy::FailClosed).unwrap();
    let mut resources = Resources::default();
    let first = registry.acquire(Some(""), &mut resources).unwrap();
    for identity in ["missing", " ", "\0"] {
        assert_eq!(
            registry.acquire(Some(identity), &mut resources),
            Err(SessionError::Stale(identity.into()))
        );
    }
    assert_eq!(resources.next, 1);
    registry.advance_ticks(1);
    assert_eq!(
        registry.acquire(Some(&first.id), &mut resources),
        Err(SessionError::Stale(first.id.clone()))
    );
    assert!(registry.entries().is_empty());
    assert_eq!(resources.released, vec![first.value]);
    assert!(registry.acquire(None, &mut resources).is_ok());
}

#[test]
fn rotation_preserves_resource_and_old_identity_is_revoked() {
    let mut registry = registry();
    let mut resources = Resources::default();
    let first = registry.acquire(None, &mut resources).unwrap();
    registry.advance_ticks(7);
    let rotated = registry.rotate(&first.id, &mut resources).unwrap();
    assert_eq!(rotated.value, first.value);
    assert_ne!(rotated.id, first.id);
    assert_eq!(rotated.expires_at_tick, 17);
    assert_eq!(
        registry.live(&first.id, &mut resources),
        Err(SessionError::Stale(first.id.clone()))
    );
    assert_eq!(
        registry.expire(&first.id, &mut resources),
        Err(SessionError::Stale(first.id.clone()))
    );
    assert_eq!(
        registry.rotate(&first.id, &mut resources),
        Err(SessionError::Stale(first.id.clone()))
    );
    registry.expire(&rotated.id, &mut resources).unwrap();
    assert!(registry.entries().is_empty());
    assert_eq!(resources.released, vec![first.value]);
}

#[test]
fn failed_entropy_or_resource_allocation_cannot_modify_existing_sessions() {
    let mut registry = registry();
    let mut resources = Resources::default();
    let first = registry.acquire(None, &mut resources).unwrap();
    registry.advance_ticks(4);
    assert_eq!(
        registry.rotate_with(&first.id, &mut resources, |_| Err(SessionError::Identity(
            terlan_runtime_abi::NativeAdapterError::new(
                "http.session.entropy",
                "entropy failed",
                0
            )
        ))),
        Err(SessionError::Identity(
            terlan_runtime_abi::NativeAdapterError::new(
                "http.session.entropy",
                "entropy failed",
                0
            )
        ))
    );
    assert_eq!(registry.entries()[&first.id], first);
    resources.fail_create = true;
    assert_eq!(
        registry.acquire(None, &mut resources),
        Err(SessionError::Resource("allocation failed".into()))
    );
    assert_eq!(registry.entries().len(), 1);
    assert_eq!(registry.entries()[&first.id], first);
    assert!(resources.released.is_empty());
}

#[test]
fn failed_release_keeps_entry_for_retry_on_every_cleanup_path() {
    for path in 0..5 {
        let mut registry = registry();
        let mut resources = Resources::default();
        let first = registry.acquire(None, &mut resources).unwrap();
        if path != 0 {
            registry.advance_ticks(10);
        }
        resources.fail_release = true;
        let error = match path {
            0 => registry.expire(&first.id, &mut resources).unwrap_err(),
            1 => registry.live(&first.id, &mut resources).unwrap_err(),
            2 => registry
                .acquire(Some(&first.id), &mut resources)
                .unwrap_err(),
            3 => registry.expire_due(&mut resources).unwrap_err(),
            _ => registry.rotate(&first.id, &mut resources).unwrap_err(),
        };
        assert_eq!(error, SessionError::Resource("cleanup failed".into()));
        assert_eq!(registry.entries()[&first.id], first);
        assert_eq!(resources.next, 1);
        resources.fail_release = false;
        registry.advance_ticks(10);
        assert_eq!(registry.expire_due(&mut resources).unwrap(), vec![first.id]);
        assert!(registry.entries().is_empty());
        assert_eq!(resources.released, vec![first.value]);
    }
}

#[test]
fn exited_resources_are_reclaimed_without_reusing_the_identity() {
    let mut registry = registry();
    let mut resources = Resources::default();
    let first = registry.acquire(None, &mut resources).unwrap();
    resources.live.clear();
    assert!(!registry.is_live(&first.id, |value| resources.live.contains(value)));
    assert_eq!(
        registry.live(&first.id, &mut resources),
        Err(SessionError::Resource("resource exited".into()))
    );
    assert!(registry.entries().is_empty());
    assert_eq!(resources.released, vec![first.value]);
    let next = registry.acquire(Some(&first.id), &mut resources).unwrap();
    assert_ne!(next.id, first.id);
    resources.live.clear();
    let replacement = registry.acquire(Some(&next.id), &mut resources).unwrap();
    assert_ne!(replacement.value, next.value);
    assert_eq!(resources.released, vec![first.value, next.value]);
}

#[test]
fn restore_rejects_expired_empty_and_duplicate_entries_without_overwrite() {
    let mut registry = registry();
    let entry = SessionEntry {
        id: "restored".into(),
        expires_at_tick: 15,
        value: 42,
    };
    registry.advance_ticks(10);
    registry.restore(entry.clone()).unwrap();
    let invalid = [
        SessionEntry {
            id: "".into(),
            ..entry.clone()
        },
        SessionEntry {
            expires_at_tick: 10,
            ..entry.clone()
        },
        SessionEntry {
            expires_at_tick: 9,
            ..entry.clone()
        },
        SessionEntry {
            value: 99,
            ..entry.clone()
        },
    ];
    for invalid in invalid {
        assert!(registry.restore(invalid).is_err());
        assert_eq!(registry.entries().len(), 1);
        assert_eq!(registry.entries()[&entry.id], entry);
    }
    *registry.value_mut(&entry.id).unwrap() = 43;
    assert_eq!(registry.entries()[&entry.id].value, 43);
    assert_eq!(registry.entries()[&entry.id].expires_at_tick, 15);
    assert!(registry.value_mut("missing").is_none());
}

#[test]
fn clock_and_deadlines_saturate_without_reviving_expired_entries() {
    assert!(SessionRegistry::<u64>::new(0, RecoveryPolicy::FailClosed).is_err());
    let mut registry =
        SessionRegistry::new(u64::MAX, RecoveryPolicy::CreateLocalReplacement).unwrap();
    let mut resources = Resources::default();
    registry.advance_ticks(1);
    let first = registry.acquire(None, &mut resources).unwrap();
    assert_eq!(first.expires_at_tick, u64::MAX);
    registry.advance_ticks(u64::MAX);
    registry.advance_ticks(1);
    assert_eq!(registry.now_tick(), u64::MAX);
    assert!(!registry.is_live(&first.id, |_| true));
    registry.expire_due(&mut resources).unwrap();
    assert!(registry.expire_due(&mut resources).unwrap().is_empty());
}

#[test]
fn typed_errors_preserve_existing_adapter_diagnostics() {
    let cases = [
        (
            SessionError::ZeroTtl,
            "HTTP session TTL must be greater than 0",
        ),
        (SessionError::Stale("a".into()), "stale HTTP session `a`"),
        (
            SessionError::EmptySnapshotIdentity,
            "HTTP session persistence snapshot id cannot be empty",
        ),
        (
            SessionError::ExpiredSnapshot("a".into()),
            "HTTP session persistence snapshot `a` is expired",
        ),
        (
            SessionError::DuplicateSnapshot("a".into()),
            "HTTP session persistence snapshot `a` would overwrite live session",
        ),
        (SessionError::Resource("host failed".into()), "host failed"),
        (
            SessionError::Identity(terlan_runtime_abi::NativeAdapterError::new(
                "http.session.entropy",
                "unavailable",
                0,
            )),
            "error[http.session.entropy]: unavailable",
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
    }
}

#[test]
fn monotonic_clock_is_opt_in_idempotent_and_saturates() {
    let mut registry = registry();
    let now = Instant::now();
    registry.advance_ticks(7);
    assert_eq!(
        registry.observed_tick(now + std::time::Duration::from_secs(100)),
        7
    );
    registry.start_clock();
    let origin = registry.clock_origin.unwrap();
    registry.start_clock();
    assert_eq!(registry.clock_origin, Some(origin));
    assert_eq!(registry.observed_tick(origin), 7);
    assert_eq!(
        registry.observed_tick(origin + std::time::Duration::from_millis(999)),
        7
    );
    assert_eq!(
        registry.observed_tick(origin + std::time::Duration::from_secs(2)),
        9
    );
    assert_eq!(registry.observed_tick(now), 7);
    registry.advance_ticks(u64::MAX);
    assert_eq!(
        registry.observed_tick(origin + std::time::Duration::from_secs(2)),
        u64::MAX
    );
}

#[test]
fn indexed_expiry_bounds_cleanup_and_rotation_does_not_leave_old_deadlines() {
    let mut registry = registry();
    let mut resources = Resources::default();
    let mut rotating = registry.acquire(None, &mut resources).unwrap();
    for _ in 0..50 {
        rotating = registry.rotate(&rotating.id, &mut resources).unwrap();
        assert_eq!(registry.deadlines.len(), 1);
    }
    registry.advance_ticks(1);
    let later = registry.acquire(None, &mut resources).unwrap();
    registry.advance_ticks(9);
    assert!(registry
        .expire_due_limit(&mut resources, 0)
        .unwrap()
        .is_empty());
    assert_eq!(registry.deadlines.len(), 2);
    assert_eq!(
        registry.expire_due_limit(&mut resources, 1).unwrap(),
        vec![rotating.id]
    );
    assert_eq!(registry.deadlines.len(), 1);
    assert!(registry
        .expire_due_limit(&mut resources, 1)
        .unwrap()
        .is_empty());
    registry.advance_ticks(1);
    resources.fail_release = true;
    assert!(registry.expire_due_limit(&mut resources, 1).is_err());
    assert_eq!(registry.deadlines.len(), 1);
    resources.fail_release = false;
    assert_eq!(
        registry.expire_due_limit(&mut resources, 1).unwrap(),
        vec![later.id]
    );
    assert!(registry.deadlines.is_empty());
    assert_eq!(resources.released, vec![rotating.value, later.value]);
}

#[test]
fn clocked_reads_reject_expired_entries_before_host_cleanup_runs() {
    let mut registry = registry();
    let mut resources = Resources::default();
    let first = registry.acquire(None, &mut resources).unwrap();
    registry.clock_origin = Some(Instant::now() - std::time::Duration::from_secs(11));
    assert!(!registry.is_live(&first.id, |_| true));
    assert_eq!(registry.entries().len(), 1, "timer has not run yet");
    assert_eq!(
        registry.live(&first.id, &mut resources),
        Err(SessionError::Stale(first.id))
    );
    assert_eq!(resources.released, vec![first.value]);
    assert!(registry.entries().is_empty());
    assert!(registry.deadlines.is_empty());
}
