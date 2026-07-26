//! Process-wide invalidation generation for owner-local handler decisions.

use std::sync::atomic::{AtomicU64, Ordering};

static HANDLER_CACHE_EPOCH: AtomicU64 = AtomicU64::new(1);

pub(in crate::commands::serve) fn current() -> u64 {
    HANDLER_CACHE_EPOCH.load(Ordering::Acquire)
}

pub(super) fn advance() {
    HANDLER_CACHE_EPOCH.fetch_add(1, Ordering::AcqRel);
}
