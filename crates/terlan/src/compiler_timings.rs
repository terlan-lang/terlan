//! Process-local activation for complete compiler phase timing.

use std::sync::atomic::{AtomicUsize, Ordering};

const FRONTEND_TIMINGS_ENVIRONMENT: &str = "TERLAN_COMPILER_PHASE_TIMINGS";
const NATIVE_TIMINGS_ENVIRONMENT: &str = "TERLAN_NATIVE_AOT_TIMINGS";

static ACTIVE_SCOPES: AtomicUsize = AtomicUsize::new(0);

/// Keeps command-line timing enabled for one dispatcher invocation.
pub(crate) struct TimingScope {
    active: bool,
}

impl Drop for TimingScope {
    fn drop(&mut self) {
        if self.active {
            ACTIVE_SCOPES.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Activates detailed timing for the lifetime of one CLI command.
pub(crate) fn scope(active: bool) -> TimingScope {
    if active {
        ACTIVE_SCOPES.fetch_add(1, Ordering::Relaxed);
    }
    TimingScope { active }
}

/// Returns whether frontend and typecheck timing is enabled.
pub(crate) fn frontend_enabled() -> bool {
    ACTIVE_SCOPES.load(Ordering::Relaxed) > 0
        || std::env::var_os(FRONTEND_TIMINGS_ENVIRONMENT).as_deref()
            == Some(std::ffi::OsStr::new("1"))
}

/// Returns whether native lowering and AOT timing is enabled.
pub(crate) fn native_enabled() -> bool {
    frontend_enabled() || std::env::var_os(NATIVE_TIMINGS_ENVIRONMENT).is_some()
}

#[cfg(test)]
#[path = "compiler_timings_test.rs"]
mod compiler_timings_test;
