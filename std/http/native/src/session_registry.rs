//! Package-owned session identity and lifetime policy over host-owned resources.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

#[path = "session_registry_error.rs"]
mod error;
pub use error::SessionError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryPolicy {
    CreateLocalReplacement,
    FailClosed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SessionEntry<T> {
    pub id: String,
    pub expires_at_tick: u64,
    pub value: T,
}

/// Hosts supply resource mechanics, not identity, expiry, or recovery policy.
/// Release must tolerate resources that have already exited and support retry
/// after an error. Failed cleanup leaves the registry entry available for retry.
pub trait SessionResources<T> {
    fn create(&mut self, identity: &str) -> Result<T, SessionError>;
    fn failure(&self, identity: &str, value: &T) -> Option<SessionError>;
    fn release(&mut self, value: &T) -> Result<(), SessionError>;
}

#[derive(Debug)]
pub struct SessionRegistry<T> {
    entries: BTreeMap<String, SessionEntry<T>>,
    deadlines: BTreeSet<(u64, String)>,
    now_tick: u64,
    clock_origin: Option<Instant>,
    recovery: RecoveryPolicy,
}

impl<T: Clone> SessionRegistry<T> {
    pub fn new(recovery: RecoveryPolicy) -> Self {
        Self {
            entries: BTreeMap::new(),
            deadlines: BTreeSet::new(),
            now_tick: 0,
            clock_origin: None,
            recovery,
        }
    }

    pub fn entries(&self) -> &BTreeMap<String, SessionEntry<T>> {
        &self.entries
    }

    /// Mutable application state cannot change registry identity or expiry.
    pub fn value_mut(&mut self, identity: &str) -> Option<&mut T> {
        self.entries.get_mut(identity).map(|entry| &mut entry.value)
    }

    /// One production tick is one monotonic second. Repeated attachment of a
    /// host or image must not restart the application clock.
    pub fn start_clock(&mut self) {
        self.clock_origin.get_or_insert_with(Instant::now);
    }

    pub fn now_tick(&self) -> u64 {
        self.observed_tick(Instant::now())
    }

    fn observed_tick(&self, now: Instant) -> u64 {
        self.now_tick.saturating_add(
            self.clock_origin
                .map_or(0, |origin| now.saturating_duration_since(origin).as_secs()),
        )
    }

    pub fn advance_ticks(&mut self, ticks: u64) {
        self.now_tick = self.now_tick.saturating_add(ticks);
    }

    /// Looks up an exact identity without allocating a replacement. Stale
    /// resources are released before reporting absence or fail-closed policy.
    pub fn lookup(
        &mut self,
        identity: &str,
        resources: &mut impl SessionResources<T>,
    ) -> Result<Option<SessionEntry<T>>, SessionError> {
        if !identity.is_empty() {
            if self.is_live(identity, |value| {
                resources.failure(identity, value).is_none()
            }) {
                return Ok(Some(self.entries[identity].clone()));
            }
            self.remove(identity, resources)?;
            if self.recovery == RecoveryPolicy::FailClosed {
                return Err(SessionError::Stale(identity.into()));
            }
        }
        Ok(None)
    }

    /// Allocates a fresh identity, never accepting a caller-selected identity.
    pub fn create(
        &mut self,
        excluded_identity: &str,
        ttl_ticks: u64,
        resources: &mut impl SessionResources<T>,
    ) -> Result<SessionEntry<T>, SessionError> {
        self.create_with(ttl_ticks, resources, |registry| {
            registry.issue(excluded_identity)
        })
    }

    fn create_with(
        &mut self,
        ttl_ticks: u64,
        resources: &mut impl SessionResources<T>,
        issue: impl FnOnce(&Self) -> Result<String, SessionError>,
    ) -> Result<SessionEntry<T>, SessionError> {
        if ttl_ticks == 0 {
            return Err(SessionError::ZeroTtl);
        }
        let id = issue(self)?;
        let value = resources.create(&id)?;
        let entry = SessionEntry {
            id,
            expires_at_tick: self.now_tick().saturating_add(ttl_ticks),
            value,
        };
        self.insert(entry.clone());
        Ok(entry)
    }

    // Historical registry scenarios compose the two operations just as source
    // does. Production callers cannot bypass the source acquisition branch.
    #[cfg(test)]
    fn acquire(
        &mut self,
        identity: Option<&str>,
        ttl_ticks: u64,
        resources: &mut impl SessionResources<T>,
    ) -> Result<SessionEntry<T>, SessionError> {
        let identity = identity.unwrap_or_default();
        match self.lookup(identity, resources)? {
            Some(entry) => Ok(entry),
            None => self.create(identity, ttl_ticks, resources),
        }
    }

    pub fn is_live(&self, identity: &str, alive: impl FnOnce(&T) -> bool) -> bool {
        self.entries
            .get(identity)
            .is_some_and(|entry| entry.expires_at_tick > self.now_tick() && alive(&entry.value))
    }

    pub fn live(
        &mut self,
        identity: &str,
        resources: &mut impl SessionResources<T>,
    ) -> Result<SessionEntry<T>, SessionError> {
        let entry = self
            .entries
            .get(identity)
            .ok_or_else(|| SessionError::Stale(identity.into()))?;
        let failure = if entry.expires_at_tick <= self.now_tick() {
            Some(SessionError::Stale(identity.into()))
        } else {
            resources.failure(identity, &entry.value)
        };
        if let Some(error) = failure {
            self.remove(identity, resources)?;
            return Err(error);
        }
        Ok(entry.clone())
    }

    pub fn rotate(
        &mut self,
        identity: &str,
        ttl_ticks: u64,
        resources: &mut impl SessionResources<T>,
    ) -> Result<SessionEntry<T>, SessionError> {
        self.rotate_with(identity, ttl_ticks, resources, |registry| {
            registry.issue(identity)
        })
    }

    fn rotate_with(
        &mut self,
        identity: &str,
        ttl_ticks: u64,
        resources: &mut impl SessionResources<T>,
        issue: impl FnOnce(&Self) -> Result<String, SessionError>,
    ) -> Result<SessionEntry<T>, SessionError> {
        if ttl_ticks == 0 {
            return Err(SessionError::ZeroTtl);
        }
        self.live(identity, resources)?;
        let id = issue(self)?;
        let mut entry = self
            .entries
            .remove(identity)
            .expect("validated live session");
        self.deadlines
            .remove(&(entry.expires_at_tick, entry.id.clone()));
        entry.id = id;
        entry.expires_at_tick = self.now_tick().saturating_add(ttl_ticks);
        self.insert(entry.clone());
        Ok(entry)
    }

    pub fn expire(
        &mut self,
        identity: &str,
        resources: &mut impl SessionResources<T>,
    ) -> Result<(), SessionError> {
        self.live(identity, resources)?;
        self.remove(identity, resources)
    }

    pub fn expire_due(
        &mut self,
        resources: &mut impl SessionResources<T>,
    ) -> Result<Vec<String>, SessionError> {
        let mut expired = self.expire_due_limit(resources, usize::MAX)?;
        expired.sort();
        Ok(expired)
    }

    /// Limit resource releases per pass; each release's cost remains host-owned.
    pub fn expire_due_limit(
        &mut self,
        resources: &mut impl SessionResources<T>,
        limit: usize,
    ) -> Result<Vec<String>, SessionError> {
        let now = self.now_tick();
        let mut expired = Vec::new();
        for _ in 0..limit {
            let Some((deadline, identity)) = self.deadlines.first() else {
                break;
            };
            if *deadline > now {
                break;
            }
            let identity = identity.clone();
            self.remove(&identity, resources)?;
            expired.push(identity);
        }
        Ok(expired)
    }

    /// Validate a durable identity before the host allocates restoration resources.
    pub fn validate_restore(
        &self,
        identity: &str,
        expires_at_tick: u64,
    ) -> Result<(), SessionError> {
        if identity.is_empty() {
            return Err(SessionError::EmptySnapshotIdentity);
        }
        if expires_at_tick <= self.now_tick() {
            return Err(SessionError::ExpiredSnapshot(identity.into()));
        }
        if self.entries.contains_key(identity) {
            return Err(SessionError::DuplicateSnapshot(identity.into()));
        }
        Ok(())
    }

    pub fn restore(&mut self, entry: SessionEntry<T>) -> Result<(), SessionError> {
        self.validate_restore(&entry.id, entry.expires_at_tick)?;
        self.insert(entry);
        Ok(())
    }

    fn remove(
        &mut self,
        identity: &str,
        resources: &mut impl SessionResources<T>,
    ) -> Result<(), SessionError> {
        if let Some(entry) = self.entries.get(identity) {
            resources.release(&entry.value)?;
            self.deadlines
                .remove(&(entry.expires_at_tick, entry.id.clone()));
            self.entries.remove(identity);
        }
        Ok(())
    }

    fn insert(&mut self, entry: SessionEntry<T>) {
        self.deadlines
            .insert((entry.expires_at_tick, entry.id.clone()));
        self.entries.insert(entry.id.clone(), entry);
    }

    fn issue(&self, excluded: &str) -> Result<String, SessionError> {
        crate::session_identity::issue(excluded, |identity| self.entries.contains_key(identity))
            .map_err(SessionError::Identity)
    }
}

#[cfg(test)]
#[path = "session_registry_test.rs"]
mod tests;
