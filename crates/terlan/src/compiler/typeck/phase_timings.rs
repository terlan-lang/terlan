//! Opt-in timing for typechecker development and build-cost attribution.

use std::time::Instant;

const PHASE_TIMINGS_ENVIRONMENT: &str = "TERLAN_COMPILER_PHASE_TIMINGS";

/// Local typecheck phase clock; disabled clocks only retain two instants.
pub(super) struct TypeCheckTimings {
    enabled: bool,
    started: Instant,
    last: Instant,
}

impl TypeCheckTimings {
    pub(super) fn new() -> Self {
        let now = Instant::now();
        Self {
            enabled: std::env::var_os(PHASE_TIMINGS_ENVIRONMENT).as_deref()
                == Some(std::ffi::OsStr::new("1")),
            started: now,
            last: now,
        }
    }

    pub(super) fn mark(&mut self, phase: &str) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        eprintln!(
            "terlc timing: typecheck.{phase}: +{}ms total={}ms",
            now.duration_since(self.last).as_millis(),
            now.duration_since(self.started).as_millis()
        );
        self.last = now;
    }
}
